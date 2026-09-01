use expect_test::expect;

use super::ParityBatchCase;

fn auto_minor_mode_set_auto_mode_advice_is_installed_exactly_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_set_auto_mode_advice_is_installed_exactly_once",
        r##"(let ((count 0)
                                members)
                           (advice-mapc
                            (lambda (advice properties)
                              (when
                                  (eq advice
                                      #'auto-minor-mode-set)
                                (setq count
                                      (1+ count))
                                (push
                                 (list
                                  advice
                                  properties)
                                 members)))
                           #'set-auto-mode)
                           (list
                            (and
                             (advice-member-p
                              #'auto-minor-mode-set
                              #'set-auto-mode)
                             t)
                            count
                            members))"##,
        expect!["OK (t 1 ((auto-minor-mode-set nil)))"],
    )
}

fn auto_minor_mode_real_set_auto_mode_selects_major_then_filename_minor_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_real_set_auto_mode_selects_major_then_filename_minor_modes",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert "(message \"hello\")\n")
                           (setq
                            buffer-file-name
                            "/project/midnight-theme.el"
                            auto-mode-alist
                            '(("\\.el\\'" . emacs-lisp-mode))
                            auto-minor-mode-alist
                            '(("-theme\\.el\\'"
                               . auto-minor-mode-test-alpha-mode)
                              ("\\.el\\'"
                               . auto-minor-mode-test-beta-mode)))
                           (set-auto-mode)
                           (list
                            major-mode
                            auto-minor-mode-test-alpha-mode
                            auto-minor-mode-test-beta-mode
                            (nreverse
                             auto-minor-mode-test-events)))"##,
        expect![
            "OK (emacs-lisp-mode t t ((:alpha 1 t 19 emacs-lisp-mode) (:beta 1 t 19 emacs-lisp-mode)))"
        ],
    )
}

fn auto_minor_mode_real_set_auto_mode_reactivation_obeys_keep_flag() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_real_set_auto_mode_reactivation_obeys_keep_flag",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (setq
                            buffer-file-name
                            "/project/service.ammtest.el"
                            auto-mode-alist
                            '(("\\.el\\'"
                               . emacs-lisp-mode))
                            auto-minor-mode-alist
                            '(("\\.ammtest\\.el\\'"
                               . auto-minor-mode-test-alpha-mode)))
                           (set-auto-mode)
                           (let ((first
                                  (length
                                   auto-minor-mode-test-events)))
                             (set-auto-mode)
                             (let ((second
                                    (length
                                     auto-minor-mode-test-events)))
                               (set-auto-mode t)
                               (let ((kept
                                      (length
                                       auto-minor-mode-test-events)))
                                 (set-auto-mode)
                                 (list
                                  first
                                  second
                                  kept
                                  (length
                                   auto-minor-mode-test-events)
                                  (nreverse
                                   auto-minor-mode-test-events))))))"##,
        expect![
            "OK (1 2 2 3 ((:alpha 1 t 1 emacs-lisp-mode) (:alpha 1 t 1 emacs-lisp-mode) (:alpha 1 t 1 emacs-lisp-mode)))"
        ],
    )
}

fn auto_minor_mode_same_mode_matching_filename_and_magic_runs_twice_unless_kept() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_minor_mode_same_mode_matching_filename_and_magic_runs_twice_unless_kept",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert "MAGIC service\n")
                           (setq
                            buffer-file-name
                            "/project/service.cfg"
                            auto-mode-alist
                            nil
                            auto-minor-mode-alist
                            '(("\\.cfg\\'"
                               . auto-minor-mode-test-alpha-mode))
                            auto-minor-mode-magic-alist
                            '(("\\`MAGIC"
                               . auto-minor-mode-test-alpha-mode)))
                           (auto-minor-mode-set)
                           (let ((ordinary
                                  (nreverse
                                   auto-minor-mode-test-events)))
                             (setq auto-minor-mode-test-events nil)
                             (auto-minor-mode-set t)
                             (list
                              ordinary
                              (nreverse
                               auto-minor-mode-test-events)
                              auto-minor-mode-test-alpha-mode)))"##,
        expect!["OK (((:alpha 1 t 15 fundamental-mode) (:alpha 1 t 1 fundamental-mode)) nil t)"],
    )
}

fn auto_minor_mode_filename_rules_run_before_magic_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_filename_rules_run_before_magic_rules",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert "HEADER\n")
                           (setq
                            buffer-file-name
                            "/project/service.cfg"
                            auto-mode-alist
                            nil
                            auto-minor-mode-alist
                            '(("\\.cfg\\'"
                               . auto-minor-mode-test-alpha-mode))
                            auto-minor-mode-magic-alist
                            (list
                             (cons
                              (lambda ()
                                auto-minor-mode-test-alpha-mode)
                              'auto-minor-mode-test-beta-mode)))
                           (auto-minor-mode-set)
                           (list
                            auto-minor-mode-test-alpha-mode
                            auto-minor-mode-test-beta-mode
                            (nreverse
                             auto-minor-mode-test-events)))"##,
        expect!["OK (t t ((:alpha 1 t 8 fundamental-mode) (:beta 1 t 1 fundamental-mode)))"],
    )
}

fn auto_minor_mode_advice_can_be_removed_and_restored_without_leaking_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_advice_can_be_removed_and_restored_without_leaking_state",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (setq
                            buffer-file-name
                            "/project/service.cfg"
                            auto-mode-alist
                            nil
                            auto-minor-mode-alist
                            '(("\\.cfg\\'"
                               . auto-minor-mode-test-alpha-mode)))
                           (unwind-protect
                               (progn
                                 (advice-remove
                                  #'set-auto-mode
                                  #'auto-minor-mode-set)
                                 (set-auto-mode)
                                 (let ((without
                                        (list
                                         auto-minor-mode-test-alpha-mode
                                         auto-minor-mode-test-events)))
                                   (advice-add
                                    #'set-auto-mode
                                    :after
                                    #'auto-minor-mode-set)
                                   (auto-minor-mode-test-reset)
                                   (set-auto-mode)
                                   (list
                                    without
                                    auto-minor-mode-test-alpha-mode
                                    (nreverse
                                     auto-minor-mode-test-events)
                                    (and
                                     (advice-member-p
                                      #'auto-minor-mode-set
                                      #'set-auto-mode)
                                     t))))
                             (unless
                                 (advice-member-p
                                  #'auto-minor-mode-set
                                  #'set-auto-mode)
                               (advice-add
                                #'set-auto-mode
                                :after
                                #'auto-minor-mode-set))))"##,
        expect!["OK ((nil nil) t ((:alpha 1 t 1 fundamental-mode)) t)"],
    )
    .fresh_process()
}

fn auto_minor_mode_reloading_source_keeps_single_advice_and_deferred_integration() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_minor_mode_reloading_source_keeps_single_advice_and_deferred_integration",
        r##"(let ((count-advice
                                (lambda ()
                                  (let ((count 0))
                                    (advice-mapc
                                     (lambda (advice _properties)
                                       (when
                                           (eq
                                            advice
                                            #'auto-minor-mode-set)
                                         (setq count
                                               (1+ count))))
                                     #'set-auto-mode)
                                    count))))
                           (let ((before
                                  (funcall count-advice)))
                             (load
                              (getenv
                               "NEOMACS_PACKAGE_SOURCE")
                              nil
                              t
                              t)
                             (list
                              before
                              (funcall count-advice)
                              (featurep
                               'auto-minor-mode)
                              (and
                               (assq
                                'use-package
                                after-load-alist)
                               t))))"##,
        expect!["OK (1 1 t t)"],
    )
}

pub(super) fn advice_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_minor_mode_set_auto_mode_advice_is_installed_exactly_once(),
        auto_minor_mode_real_set_auto_mode_selects_major_then_filename_minor_modes(),
        auto_minor_mode_real_set_auto_mode_reactivation_obeys_keep_flag(),
        auto_minor_mode_same_mode_matching_filename_and_magic_runs_twice_unless_kept(),
        auto_minor_mode_filename_rules_run_before_magic_rules(),
        auto_minor_mode_advice_can_be_removed_and_restored_without_leaking_state(),
        auto_minor_mode_reloading_source_keeps_single_advice_and_deferred_integration(),
    ]
}
