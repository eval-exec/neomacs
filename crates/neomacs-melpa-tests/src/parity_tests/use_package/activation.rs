use expect_test::expect;

use super::ParityBatchCase;

fn use_package_commands_create_interactive_autoloads_without_loading_the_feature() -> ParityBatchCase
{
    ParityBatchCase::value(
        "use_package_commands_create_interactive_autoloads_without_loading_the_feature",
        r##"(progn
               (fmakunbound 'neomacs-use-package-command-one)
               (fmakunbound 'neomacs-use-package-command-two)
               (use-package neomacs-use-package-command-library
                 :commands
                 (neomacs-use-package-command-one
                  neomacs-use-package-command-two))
               (mapcar
                (lambda (symbol)
                  (let ((definition (symbol-function symbol)))
                    (list
                     symbol
                     (autoloadp definition)
                     (nth 1 definition)
                     (nth 4 definition)
                     (commandp symbol))))
                '(neomacs-use-package-command-one
                  neomacs-use-package-command-two)))"##,
        expect![[
            r#"OK ((neomacs-use-package-command-one t "neomacs-use-package-command-library" nil t) (neomacs-use-package-command-two t "neomacs-use-package-command-library" nil t))"#
        ]],
    )
}

fn use_package_hooks_apply_suffixes_autoload_symbols_and_run_lambda_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_hooks_apply_suffixes_autoload_symbols_and_run_lambda_entries",
        r##"(let ((neomacs-use-package-mode-hook nil)
                    events)
               (fmakunbound 'neomacs-use-package-hook-function)
               (use-package neomacs-use-package-hook-library
                 :hook
                 ((neomacs-use-package-mode
                   . neomacs-use-package-hook-function)
                  (neomacs-use-package-mode
                   . (lambda () (push 'lambda events)))))
               (let ((before
                      (list
                       (mapcar
                        (lambda (function)
                          (if (symbolp function)
                              (list
                               function
                               (autoloadp
                                (symbol-function function)))
                            (car-safe function)))
                        neomacs-use-package-mode-hook))))
                 (fset 'neomacs-use-package-hook-function
                       (lambda () (push 'symbol events)))
                 (run-hooks 'neomacs-use-package-mode-hook)
                 (list before (nreverse events))))"##,
        expect![[r#"OK ((nil) (lambda symbol))"#]],
    )
}

fn use_package_mode_interpreter_magic_and_fallback_register_exact_alist_entries() -> ParityBatchCase
{
    ParityBatchCase::value(
        "use_package_mode_interpreter_magic_and_fallback_register_exact_alist_entries",
        r##"(let ((auto-mode-alist nil)
                    (interpreter-mode-alist nil)
                    (magic-mode-alist nil)
                    (magic-fallback-mode-alist nil))
               (use-package neomacs-use-package-detect
                 :no-require t
                 :mode
                 (("\\.neo\\'" . neomacs-use-package-detect-mode)
                  ("\\.neo2\\'" . neomacs-use-package-detect-mode))
                 :interpreter
                 (("neo" . neomacs-use-package-detect-mode)
                  ("neo2" . neomacs-use-package-detect-mode))
                 :magic
                 (("NEO!" . neomacs-use-package-detect-mode))
                 :magic-fallback
                 (("fallback" . neomacs-use-package-detect-mode)))
               (list
                auto-mode-alist
                interpreter-mode-alist
                magic-mode-alist
                magic-fallback-mode-alist
                (autoloadp
                 (symbol-function
                  'neomacs-use-package-detect-mode))))"##,
        expect![[
            r##"OK ((("\\.neo2\\'" . neomacs-use-package-detect-mode) ("\\.neo\\'" . neomacs-use-package-detect-mode)) (("neo2" . neomacs-use-package-detect-mode) ("neo" . neomacs-use-package-detect-mode)) (("NEO!" . neomacs-use-package-detect-mode)) (("fallback" . neomacs-use-package-detect-mode)) t)"##
        ]],
    )
}

fn use_package_after_all_waits_for_every_feature_in_normalized_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_after_all_waits_for_every_feature_in_normalized_order",
        r##"(progn
               (defvar neomacs-use-package-events nil)
               (setq neomacs-use-package-events nil)
               (use-package neomacs-use-package-after-all-target
                 :no-require t
                 :after
                 (:all neomacs-use-package-after-one
                       neomacs-use-package-after-two)
                 :config
                 (push 'configured neomacs-use-package-events))
               (let ((initial neomacs-use-package-events))
                 (provide 'neomacs-use-package-after-one)
                 (let ((after-one neomacs-use-package-events))
                   (provide 'neomacs-use-package-after-two)
                   (list
                    initial
                    after-one
                    (nreverse
                     neomacs-use-package-events)))))"##,
        expect![[r#"OK (nil nil (configured))"#]],
    )
}

fn use_package_after_any_runs_once_when_the_first_of_several_features_loads() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_after_any_runs_once_when_the_first_of_several_features_loads",
        r##"(progn
               (defvar neomacs-use-package-events nil)
               (setq neomacs-use-package-events nil)
               (use-package neomacs-use-package-after-any-target
                 :no-require t
                 :after
                 (:any neomacs-use-package-after-left
                       neomacs-use-package-after-right)
                 :config
                 (push 'configured neomacs-use-package-events))
               (provide 'neomacs-use-package-after-right)
               (let ((after-first
                      (copy-sequence
                       neomacs-use-package-events)))
                 (provide 'neomacs-use-package-after-left)
                 (list
                  after-first
                  neomacs-use-package-events)))"##,
        expect![[r#"OK ((configured) (configured))"#]],
    )
}

fn use_package_deferred_config_runs_when_the_declared_feature_is_provided() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_deferred_config_runs_when_the_declared_feature_is_provided",
        r##"(progn
               (defvar neomacs-use-package-events nil)
               (setq neomacs-use-package-events nil)
               (use-package neomacs-use-package-deferred-feature
                 :defer t
                 :config
                 (push 'configured neomacs-use-package-events))
               (let ((before neomacs-use-package-events))
                 (provide 'neomacs-use-package-deferred-feature)
                 (list
                  before
                  (nreverse
                   neomacs-use-package-events))))"##,
        expect![[r#"OK (nil (configured))"#]],
    )
}

fn use_package_demand_loads_a_real_library_between_init_and_config() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_demand_loads_a_real_library_between_init_and_config",
        r##"(let* ((root
                    (make-temp-file "use-package-demand-" t))
                   (load-path
                    (cons root load-path))
                   events)
               (unwind-protect
                   (progn
                     (with-temp-file
                         (expand-file-name
                          "neomacs-use-package-demand.el" root)
                       (insert
                        "(setq neomacs-use-package-demand-loaded t)\n"
                        "(provide 'neomacs-use-package-demand)\n"))
                     (setq neomacs-use-package-demand-loaded nil)
                     (use-package neomacs-use-package-demand
                       :demand t
                       :init (push 'init events)
                       :config (push 'config events))
                     (list
                      (nreverse events)
                      neomacs-use-package-demand-loaded
                      (featurep
                       'neomacs-use-package-demand)))
                 (delete-directory root t)))"##,
        expect![[r#"OK ((init config) t t)"#]],
    )
}

pub(super) fn activation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        use_package_commands_create_interactive_autoloads_without_loading_the_feature(),
        use_package_hooks_apply_suffixes_autoload_symbols_and_run_lambda_entries(),
        use_package_mode_interpreter_magic_and_fallback_register_exact_alist_entries(),
        use_package_after_all_waits_for_every_feature_in_normalized_order(),
        use_package_after_any_runs_once_when_the_first_of_several_features_loads(),
        use_package_deferred_config_runs_when_the_declared_feature_is_provided(),
        use_package_demand_loads_a_real_library_between_init_and_config(),
    ]
}
