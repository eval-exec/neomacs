use expect_test::expect;

use super::ParityBatchCase;

fn auto_minor_mode_use_package_integration_is_deferred_until_feature_load() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_use_package_integration_is_deferred_until_feature_load",
        r##"(let
                             ((deferred
                               (assq
                                'use-package
                                after-load-alist)))
                           (list
                            (featurep 'use-package)
                            (fboundp
                             'use-package-normalize/:minor)
                            (fboundp
                             'use-package-handler/:minor)
                            (and deferred t)
                            (progn
                              (require 'use-package)
                              (list
                               (featurep 'use-package)
                               (fboundp
                                'use-package-normalize/:minor)
                               (fboundp
                                'use-package-normalize/:magic-minor)
                               (fboundp
                                'use-package-handler/:minor)
                               (fboundp
                                'use-package-handler/:magic-minor)
                               (and
                                (memq
                                 :minor
                                 use-package-keywords)
                                t)
                               (and
                                (memq
                                 :magic-minor
                                 use-package-keywords)
                                t)))))"##,
        expect!["OK (nil nil nil t (t t t t t t t))"],
    )
}

fn auto_minor_mode_use_package_aliases_and_handlers_have_exact_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_use_package_aliases_and_handlers_have_exact_contracts",
        r##"(progn
                           (require 'use-package)
                           (mapcar
                            (lambda (symbol)
                              (list
                               symbol
                               (help-function-arglist symbol t)
                               (documentation symbol t)
                               (eq
                                (indirect-function symbol)
                                (indirect-function
                                 (if
                                     (memq
                                      symbol
                                      '(use-package-normalize/:minor
                                        use-package-normalize/:magic-minor))
                                     'use-package-normalize-mode
                                   symbol)))
                               (file-name-nondirectory
                                (or
                                 (symbol-file symbol 'defun)
                                 ""))))
                            '(use-package-normalize/:minor
                              use-package-normalize/:magic-minor
                              use-package-handler/:minor
                              use-package-handler/:magic-minor)))"##,
        expect![[
            r#"OK ((use-package-normalize/:minor #1=(name keyword args) "Normalize arguments for keywords which add regexp/mode pairs to an alist." t "") (use-package-normalize/:magic-minor #1# "Normalize arguments for keywords which add regexp/mode pairs to an alist." t "") (use-package-handler/:minor (name _ arg rest state) nil t "") (use-package-handler/:magic-minor (name _ arg rest state) nil t ""))"#
        ]],
    )
}

fn auto_minor_mode_use_package_keywords_are_adjacent_before_commands_and_idempotent()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_use_package_keywords_are_adjacent_before_commands_and_idempotent",
        r##"(progn
                           (require 'use-package)
                           (let*
                               ((first-minor
                                 (cl-position
                                  :minor
                                  use-package-keywords))
                                (first-magic
                                 (cl-position
                                  :magic-minor
                                  use-package-keywords))
                                (commands
                                 (cl-position
                                  :commands
                                  use-package-keywords))
                                (before
                                 (copy-sequence
                                  use-package-keywords)))
                             (load
                              (getenv
                               "NEOMACS_PACKAGE_SOURCE")
                              nil
                              t
                              t)
                             (list
                              first-minor
                              first-magic
                              commands
                              (= first-magic
                                 (1+ first-minor))
                              (= commands
                                 (1+ first-magic))
                              (equal
                               before
                               use-package-keywords)
                              (cl-count
                               :minor
                               use-package-keywords)
                              (cl-count
                               :magic-minor
                               use-package-keywords)
                              (cl-subseq
                               use-package-keywords
                               (max 0 (1- first-minor))
                               (min
                                (length use-package-keywords)
                                (+ first-minor 4))))))"##,
        expect!["OK (26 27 28 t t t 1 1 (:hook :minor :magic-minor :commands :autoload))"],
    )
}

fn auto_minor_mode_use_package_handlers_delegate_to_the_correct_alists() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_use_package_handlers_delegate_to_the_correct_alists",
        r##"(progn
                           (require 'use-package)
                           (let (calls)
                             (cl-letf
                                 (((symbol-function
                                    'use-package-handle-mode)
                                   (lambda
                                       (name alist arg rest state)
                                     (push
                                      (list
                                       name
                                       alist
                                       arg
                                       rest
                                       state)
                                      calls)
                                     (list :handled alist))))
                               (list
                                (use-package-handler/:minor
                                 'demo
                                 :minor
                                 '(("\\.demo\\'"
                                    . demo-mode))
                                 '(:commands demo-command)
                                 '(:state enabled))
                                (use-package-handler/:magic-minor
                                 'demo
                                 :magic-minor
                                 '(("\\`#!demo"
                                    . demo-mode))
                                 '(:commands demo-command)
                                 '(:state enabled))
                                (nreverse calls)))))"##,
        expect![[
            r#"OK ((:handled auto-minor-mode-alist) (:handled auto-minor-mode-magic-alist) ((demo auto-minor-mode-alist (("\\.demo\\'" . demo-mode)) (:commands demo-command) (:state enabled)) (demo auto-minor-mode-magic-alist (("\\`#!demo" . demo-mode)) (:commands demo-command) (:state enabled))))"#
        ]],
    )
}

fn auto_minor_mode_real_use_package_declaration_configures_and_runs_both_rule_kinds()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_real_use_package_declaration_configures_and_runs_both_rule_kinds",
        r##"(progn
                           (require 'use-package)
                           (let
                               ((auto-minor-mode-alist nil)
                                (auto-minor-mode-magic-alist nil))
                             (eval
                              '(use-package
                                   auto-minor-mode-test-alpha-mode
                                 :no-require t
                                 :minor "\\.alpha\\'"
                                 :magic-minor "\\`#!alpha"))
                             (with-temp-buffer
                               (auto-minor-mode-test-reset)
                               (insert
                                "#!alpha\n"
                                "production=true\n")
                               (setq
                                buffer-file-name
                                "/project/service.alpha"
                                auto-mode-alist nil)
                               (set-auto-mode)
                               (list
                                auto-minor-mode-alist
                                auto-minor-mode-magic-alist
                                auto-minor-mode-test-alpha-mode
                                (nreverse
                                 auto-minor-mode-test-events)
                                major-mode))))"##,
        expect![[
            r#"OK ((("\\.alpha\\'" . auto-minor-mode-test-alpha-mode)) (("\\`#!alpha" . auto-minor-mode-test-alpha-mode)) t ((:alpha 1 t 25 fundamental-mode) (:alpha 1 t 1 fundamental-mode)) fundamental-mode)"#
        ]],
    )
}

pub(super) fn use_package_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_minor_mode_use_package_integration_is_deferred_until_feature_load(),
        auto_minor_mode_use_package_aliases_and_handlers_have_exact_contracts(),
        auto_minor_mode_use_package_keywords_are_adjacent_before_commands_and_idempotent(),
        auto_minor_mode_use_package_handlers_delegate_to_the_correct_alists(),
        auto_minor_mode_real_use_package_declaration_configures_and_runs_both_rule_kinds(),
    ]
}
