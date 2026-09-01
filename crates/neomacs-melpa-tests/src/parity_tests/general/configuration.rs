use expect_test::expect;

use super::ParityBatchCase;

fn general_public_sorters_order_mixed_key_and_state_descriptors_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_public_sorters_order_mixed_key_and_state_descriptors_exactly",
        r##"(let ((by-car
                     '((z . 1)
                       ([f2] . 2)
                       (nil . 3)
                       ("alpha" . 4)
                       (beta . 5)))
                    (by-cadr
                     '((one z)
                       (two [f2])
                       (three nil)
                       (four "alpha")
                       (five beta))))
               (list
                (general-sort-by-car
                 (copy-tree by-car))
                (general-sort-by-cadr
                 (copy-tree by-cadr))))"##,
        expect![[
            r#"OK (((nil . 3) ([f2] . 2) ("alpha" . 4) (beta . 5) (z . 1)) ((three nil) (two [f2]) (four "alpha") (five beta) (one z)))"#
        ]],
    )
}

fn general_setq_uses_custom_setters_for_defined_variables() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_setq_uses_custom_setters_for_defined_variables",
        r##"(progn
               (defcustom
                 neomacs-general-custom-value nil
                 "General parity variable."
                 :group 'general
                 :set
                 (lambda (symbol value)
                   (set-default
                    symbol
                    (list :set value))))
               (general-setq
                neomacs-general-custom-value
                'requested)
               (list
                neomacs-general-custom-value
                (default-value
                 'neomacs-general-custom-value)
                (get
                 'neomacs-general-custom-value
                 'custom-set)))"##,
        expect![[
            r#"OK (#1=(:set requested) #1# #[(symbol value) ((set-default symbol (list :set value))) (t)])"#
        ]],
    )
}

fn general_setting_helpers_preserve_default_local_and_equal_pushnew_semantics() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_setting_helpers_preserve_default_local_and_equal_pushnew_semantics",
        r##"(progn
               (defvar
                 neomacs-general-setting
                 'initial)
               (defvar
                 neomacs-general-items nil)
               (setq-default
                neomacs-general-setting
                'initial)
               (general-setq-default
                neomacs-general-setting
                'default)
               (setq
                neomacs-general-items
                '((one) (two)))
               (general-pushnew
                '(one)
                neomacs-general-items)
               (general-pushnew
                '(three)
                neomacs-general-items)
               (with-temp-buffer
                 (general-setq-local
                  neomacs-general-setting
                  'local)
                 (list
                  neomacs-general-setting
                  (default-value
                   'neomacs-general-setting)
                  neomacs-general-items
                  (local-variable-p
                   'neomacs-general-setting))))"##,
        expect![[r#"OK (local default ((three) (one) (two)) t)"#]],
    )
}

fn general_add_and_remove_hook_support_lists_order_append_and_local_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_add_and_remove_hook_support_lists_order_append_and_local_values",
        r##"(progn
               (defvar
                 neomacs-general-hook-a nil)
               (defvar
                 neomacs-general-hook-b nil)
               (setq
                neomacs-general-hook-a nil
                neomacs-general-hook-b nil)
               (general-add-hook
                '(neomacs-general-hook-a
                  neomacs-general-hook-b)
                '(forward-char backward-char))
               (general-add-hook
                'neomacs-general-hook-a
                #'next-line t)
               (let ((global
                      (list
                       neomacs-general-hook-a
                       neomacs-general-hook-b))
                     local)
                 (with-temp-buffer
                   (general-add-hook
                    'neomacs-general-hook-a
                    #'previous-line
                    t t)
                   (setq
                    local
                    neomacs-general-hook-a))
                 (general-remove-hook
                  '(neomacs-general-hook-a
                    neomacs-general-hook-b)
                  '(forward-char
                    backward-char
                    next-line))
                 (list
                  global
                  local
                  neomacs-general-hook-a
                  neomacs-general-hook-b)))"##,
        expect![[
            r#"OK (((backward-char forward-char next-line) (backward-char forward-char)) (t previous-line) nil nil)"#
        ]],
    )
}

fn general_transient_hooks_remove_after_one_run_or_first_truthy_result() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_transient_hooks_remove_after_one_run_or_first_truthy_result",
        r##"(progn
               (defvar
                 neomacs-general-once-hook nil)
               (defvar
                 neomacs-general-until-hook nil)
               (defvar
                 neomacs-general-once-count 0)
               (defvar
                 neomacs-general-until-count 0)
               (setq
                neomacs-general-once-hook nil
                neomacs-general-until-hook nil
                neomacs-general-once-count 0
                neomacs-general-until-count 0)
               (general-add-hook
                'neomacs-general-once-hook
                (lambda ()
                  (setq
                   neomacs-general-once-count
                   (1+
                    neomacs-general-once-count)))
                nil nil t)
               (general-add-hook
                'neomacs-general-until-hook
                (lambda ()
                  (setq
                   neomacs-general-until-count
                   (1+
                    neomacs-general-until-count))
                  (= neomacs-general-until-count
                     2))
                nil nil #'identity)
               (dotimes (_ 3)
                 (run-hooks
                  'neomacs-general-once-hook)
                 (run-hooks
                  'neomacs-general-until-hook))
               (list
                neomacs-general-once-count
                neomacs-general-until-count
                neomacs-general-once-hook
                neomacs-general-until-hook))"##,
        expect![[r#"OK (1 2 nil nil)"#]],
    )
}

fn general_advice_helpers_apply_lists_and_aliases_then_remove_cleanly() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_advice_helpers_apply_lists_and_aliases_then_remove_cleanly",
        r##"(progn
               (defvar
                 neomacs-general-advice-events nil)
               (setq
                neomacs-general-advice-events nil)
               (defun neomacs-general-advised-a ()
                 (push 'body-a
                       neomacs-general-advice-events)
                 'a)
               (defun neomacs-general-advised-b ()
                 (push 'body-b
                       neomacs-general-advice-events)
                 'b)
               (defun neomacs-general-before-one ()
                 (push 'before-one
                       neomacs-general-advice-events))
               (defun neomacs-general-before-two ()
                 (push 'before-two
                       neomacs-general-advice-events))
               (general-add-advice
                '(neomacs-general-advised-a
                  neomacs-general-advised-b)
                :before
                '(neomacs-general-before-one
                  neomacs-general-before-two))
               (let ((values
                      (list
                       (neomacs-general-advised-a)
                       (neomacs-general-advised-b)))
                     (events
                      (nreverse
                       neomacs-general-advice-events)))
                 (general-remove-advice
                  '(neomacs-general-advised-a
                    neomacs-general-advised-b)
                  '(neomacs-general-before-one
                    neomacs-general-before-two))
                 (setq
                  neomacs-general-advice-events nil)
                 (list
                  values
                  events
                  (neomacs-general-advised-a)
                  (nreverse
                   neomacs-general-advice-events)
                  (eq
                   (indirect-function
                    'general-add-advice)
                   (indirect-function
                    'general-advice-add))
                  (eq
                   (indirect-function
                    'general-remove-advice)
                   (indirect-function
                    'general-advice-remove)))))"##,
        expect![[
            r#"OK ((a b) (before-two before-one body-a before-two before-one body-b) a (body-a) t t)"#
        ]],
    )
}

fn general_transient_advice_removes_after_the_configured_result_condition() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_transient_advice_removes_after_the_configured_result_condition",
        r##"(progn
               (defvar
                 neomacs-general-advice-count 0)
               (setq
                neomacs-general-advice-count 0)
               (defun neomacs-general-advice-target ()
                 'original)
               (general-advice-add
                'neomacs-general-advice-target
                :override
                (lambda ()
                  (setq
                   neomacs-general-advice-count
                   (1+
                    neomacs-general-advice-count))
                  (and
                   (= neomacs-general-advice-count
                      3)
                   'finished))
                nil #'identity)
               (list
                (neomacs-general-advice-target)
                (neomacs-general-advice-target)
                (neomacs-general-advice-target)
                (neomacs-general-advice-target)
                neomacs-general-advice-count))"##,
        expect![[r#"OK (nil nil finished original 3)"#]],
    )
}

fn general_package_and_initialization_macros_run_in_the_documented_context() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_package_and_initialization_macros_run_in_the_documented_context",
        r##"(let (events)
               (general-with-package
                   'general
                 (push
                  (list
                   'package
                   general-package)
                  events))
               (let ((after-init-time t))
                 (general-after-init
                   (push 'after-init events)))
               (general-after-tty
                 (push 'after-tty events))
               (list
                (nreverse events)
                general-package
                (eq
                 (indirect-function
                  'general-with)
                 (indirect-function
                  'general-with-package))))"##,
        expect![[r#"OK (((package general) after-init after-tty) nil t)"#]],
    )
}

pub(super) fn configuration_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        general_public_sorters_order_mixed_key_and_state_descriptors_exactly(),
        general_setq_uses_custom_setters_for_defined_variables(),
        general_setting_helpers_preserve_default_local_and_equal_pushnew_semantics(),
        general_add_and_remove_hook_support_lists_order_append_and_local_values(),
        general_transient_hooks_remove_after_one_run_or_first_truthy_result(),
        general_advice_helpers_apply_lists_and_aliases_then_remove_cleanly(),
        general_transient_advice_removes_after_the_configured_result_condition(),
        general_package_and_initialization_macros_run_in_the_documented_context(),
    ]
}
