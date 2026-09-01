use expect_test::expect;

use super::ParityBatchCase;

fn auto_highlight_symbol_builtin_plugin_registry_properties_and_order_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_builtin_plugin_registry_properties_and_order_match",
        r##"(mapcar
                           (lambda (range)
                             (list
                              range
                              (symbol-value range)
                              (mapcar
                               (lambda (property)
                                 (cons
                                  property
                                  (ahs-get-plugin-prop
                                   property
                                   range
                                   t)))
                               '(name
                                 lighter
                                 face
                                 major-mode
                                 condition
                                 start
                                 end))))
                           ahs-range-plugin-list)"##,
        expect![[
            r#"OK ((ahs-range-beginning-of-defun ((name . "beginning of defun") (lighter . "HSD") (face . ahs-plugin-bod-face) (major-mode . ahs-plugin-bod-modes) (before-search lambda (symbol) (save-excursion (let ((pos (funcall ahs-plugin-bod-function))) (if (not (consp pos)) 'abort (setq ahs-plugin-bod-start (car pos)) (setq ahs-plugin-bod-end (cdr pos)))))) (start . ahs-plugin-bod-start) (end . ahs-plugin-bod-end)) ((name . "beginning of defun") (lighter . "HSD") (face . ahs-plugin-bod-face) (major-mode emacs-lisp-mode lisp-interaction-mode c++-mode c-mode) (condition . none) (start) (end))) (ahs-range-whole-buffer ((name . "whole buffer") (lighter . "HSA") (face . ahs-plugin-whole-buffer-face) (start . point-min) (end . point-max)) ((name . "whole buffer") (lighter . "HSA") (face . ahs-plugin-whole-buffer-face) (major-mode . none) (condition . none) (start . abort) (end . abort))) (ahs-range-display ((name . "display area") (lighter . "HS") (start . window-start) (end . window-end)) ((name . "display area") (lighter . "HS") (face . ahs-plugin-default-face) (major-mode . none) (condition . none) (start . abort) (end . abort))))"#
        ]],
    )
}

fn auto_highlight_symbol_runnable_plugins_filter_by_major_mode_condition_and_cycle()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_runnable_plugins_filter_by_major_mode_condition_and_cycle",
        r##"(mapcar
                           (lambda (mode)
                             (with-temp-buffer
                               (setq
                                major-mode mode
                                ahs-current-range
                                ahs-range-display)
                               (list
                                mode
                                (ahs-runnable-plugins)
                                (ahs-runnable-plugins t))))
                           '(emacs-lisp-mode
                             c-mode
                             text-mode
                             fundamental-mode))"##,
        expect![
            "OK ((emacs-lisp-mode (ahs-range-beginning-of-defun ahs-range-whole-buffer ahs-range-display) ahs-range-beginning-of-defun) (c-mode (ahs-range-beginning-of-defun ahs-range-whole-buffer ahs-range-display) ahs-range-beginning-of-defun) (text-mode (ahs-range-whole-buffer ahs-range-display) ahs-range-whole-buffer) (fundamental-mode (ahs-range-whole-buffer ahs-range-display) ahs-range-whole-buffer))"
        ],
    )
}

fn auto_highlight_symbol_custom_plugin_macro_registers_command_and_evaluates_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_custom_plugin_macro_registers_command_and_evaluates_properties",
        r##"(progn
                           (ahs-regist-range-plugin
                               fixture
                             '((name . "fixture range")
                               (lighter . "FX")
                               (face . ahs-warning-face)
                               (condition . (lambda () t))
                               (start . (lambda () (+ (point-min) 1)))
                               (end . (lambda () (- (point-max) 1))))
                             "Fixture plugin")
                           (with-temp-buffer
                             (insert "0123456789")
                             (list
                              ahs-range-plugin-list
                              ahs-range-fixture
                              (help-function-arglist
                               'ahs-chrange-fixture
                               t)
                              (commandp
                               'ahs-chrange-fixture)
                              (mapcar
                               (lambda (property)
                                 (cons
                                  property
                                  (ahs-get-plugin-prop
                                   property
                                   'ahs-range-fixture
                                   (and
                                    (eq property
                                        'face)
                                    t))))
                               '(name
                                 lighter
                                 face
                                 condition
                                 start
                                 end)))))"##,
        expect![[
            r#"OK ((ahs-range-fixture ahs-range-beginning-of-defun ahs-range-whole-buffer ahs-range-display) ((name . "fixture range") (lighter . "FX") (face . ahs-warning-face) (condition lambda nil t) (start lambda nil (+ (point-min) 1)) (end lambda nil (- (point-max) 1))) nil t ((name . "fixture range") (lighter . "FX") (face . ahs-warning-face) (condition . t) (start . 2) (end . 10)))"#
        ]],
    )
}

fn auto_highlight_symbol_plugin_property_supports_values_symbols_functions_and_abort()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_plugin_property_supports_values_symbols_functions_and_abort",
        r##"(progn
                           (defvar auto-highlight-symbol-test-value
                             17)
                           (defvar auto-highlight-symbol-test-plugin
                             '((literal . "value")
                               (symbol . auto-highlight-symbol-test-value)
                               (zero-arg . (lambda () :zero))
                               (one-arg . (lambda (arg) (list :one arg)))
                               (abort . abort)
                               (missing . nil)))
                           (mapcar
                            (lambda (case)
                              (list
                               case
                               (ahs-get-plugin-prop
                                (car case)
                                'auto-highlight-symbol-test-plugin
                                (cdr case))))
                            '((literal)
                              (symbol)
                              (zero-arg)
                              (one-arg . payload)
                              (abort)
                              (unknown))))"##,
        expect![[
            r#"OK (((literal) "value") ((symbol) 17) ((zero-arg) :zero) ((one-arg . payload) (:one payload)) ((abort) abort) ((unknown) none))"#
        ]],
    )
}

fn auto_highlight_symbol_invalid_plugin_diagnostics_cover_missing_unregistered_and_unrunnable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_invalid_plugin_diagnostics_cover_missing_unregistered_and_unrunnable",
        r##"(let ((ahs-suppress-log nil)
                                (ahs-log-echo-area-only t)
                                messages)
                           (cl-letf
                               (((symbol-function
                                  'message)
                                 (lambda (format-string &rest args)
                                   (push
                                    (apply
                                     #'format
                                     format-string
                                     args)
                                    messages))))
                             (defvar auto-highlight-symbol-test-unregistered
                               '((name . "unregistered")))
                             (defvar auto-highlight-symbol-test-unrunnable
                               '((name . "unrunnable")
                                 (major-mode . (python-mode))
                                 (condition . t)))
                             (add-to-list
                              'ahs-range-plugin-list
                              'auto-highlight-symbol-test-unrunnable)
                             (with-temp-buffer
                               (emacs-lisp-mode)
                               (list
                                (ahs-valid-plugin-p
                                 'auto-highlight-symbol-test-missing)
                                (ahs-valid-plugin-p
                                 'auto-highlight-symbol-test-unregistered)
                                (ahs-valid-plugin-p
                                 'auto-highlight-symbol-test-unrunnable)
                                (nreverse messages)))))"##,
        expect![[
            r#"OK (nil nil nil ("Plugin `auto-highlight-symbol-test-missing' doesn't exist." "Plugin `auto-highlight-symbol-test-unregistered' wrong type plugin." "Plugin `unrunnable' incorrect major-mode or condition property is `nil'."))"#
        ]],
    )
}

fn auto_highlight_symbol_change_range_updates_state_lighter_and_runs_plugin_init() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_highlight_symbol_change_range_updates_state_lighter_and_runs_plugin_init",
        r##"(progn
                           (defvar auto-highlight-symbol-test-range
                             '((name . "test")
                               (lighter . "TST")
                               (init . (lambda ()
                                         (push
                                          :init
                                          auto-highlight-symbol-test-events)))
                               (start . point-min)
                               (end . point-max)))
                           (add-to-list
                            'ahs-range-plugin-list
                            'auto-highlight-symbol-test-range)
                           (with-temp-buffer
                             (setq
                              ahs-current-range
                              ahs-range-display
                              ahs-mode-line " HS"
                              auto-highlight-symbol-test-events nil)
                             (auto-highlight-symbol-mode 1)
                             (let ((before
                                    (auto-highlight-symbol-test-mode-state)))
                               (ahs-change-range
                                'auto-highlight-symbol-test-range
                                t)
                               (list
                                before
                                (auto-highlight-symbol-test-mode-state)
                                auto-highlight-symbol-test-events
                                (ahs-decorated-current-plugin-name)))))"##,
        expect![[
            r#"OK ((t ((name . "display area") (lighter . "HS") (start . window-start) (end . window-end)) " HS" nil (ahs-start-timer t) (ahs-start-timer t) 0 0) (t ((name . "test") (lighter . "TST") (init lambda nil (push :init auto-highlight-symbol-test-events)) (start . point-min) (end . point-max)) " TST" nil nil nil 0 0) (:init) #("test" 0 4 (face ahs-plugin-default-face-unfocused)))"#
        ]],
    )
}

fn auto_highlight_symbol_beginning_of_defun_plugin_computes_real_lisp_ranges() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_beginning_of_defun_plugin_computes_real_lisp_ranges",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (insert
                            "header\n\n(defun first ()\n  1)\n\n(defun second ()\n  2)\n\nfooter")
                           (mapcar
                            (lambda (position)
                              (goto-char position)
                              (let ((builtin
                                     (save-excursion
                                       (ahs-plugin-ahs-bod)))
                                    (original
                                     (save-excursion
                                       (ahs-plugin-orignal-n2d))))
                                (list
                                 position
                                 builtin
                                 original
                                 (and
                                  (consp builtin)
                                  (buffer-substring-no-properties
                                   (car builtin)
                                   (cdr builtin))))))
                            '(1 12 27 43 60)))"##,
        expect![[
            r#"OK ((1 (1 . 8) (1 . 8) "header\n") (12 (9 . 30) (9 . 30) "(defun first ()\n  1)\n") (27 (9 . 30) (9 . 30) "(defun first ()\n  1)\n") (43 (31 . 53) (31 . 53) "(defun second ()\n  2)\n") (60 (53 . 60) (31 . 60) "\nfooter"))"#
        ]],
    )
}

fn auto_highlight_symbol_onekey_macros_install_real_key_commands_and_preserve_custom_map()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_onekey_macros_install_real_key_commands_and_preserve_custom_map",
        r##"(let ((custom-map
                                (make-sparse-keymap)))
                           (ahs-onekey-change
                            "C-c d"
                            display)
                           (ahs-onekey-change
                            "C-c w"
                            whole-buffer
                            custom-map)
                           (ahs-onekey-edit
                            "C-c e"
                            whole-buffer
                            t
                            custom-map)
                           (mapcar
                            (lambda (case)
                              (pcase-let
                                  ((`(,name ,map ,key)
                                    case))
                                (let ((definition
                                       (lookup-key
                                        map
                                        (kbd key))))
                                  (list
                                   name
                                   key
                                   definition
                                   (and
                                    (symbolp definition)
                                    (commandp definition))
                                   (and
                                    (functionp definition)
                                    (interactive-form
                                     definition))))))
                            (list
                             (list
                              'default
                              auto-highlight-symbol-mode-map
                              "C-c d")
                             (list
                              'custom-change
                              custom-map
                              "C-c w")
                             (list
                              'custom-edit
                              custom-map
                              "C-c e"))))"##,
        expect![[
            r#"OK ((default "C-c d" ahs-chrange-display t (interactive nil)) (custom-change "C-c w" ahs-chrange-whole-buffer t (interactive nil)) (custom-edit "C-c e" #[nil ((ahs-onekey-edit-function 'whole-buffer t)) (t) nil nil nil] nil (interactive nil)))"#
        ]],
    )
}

pub(super) fn plugins_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_highlight_symbol_builtin_plugin_registry_properties_and_order_match(),
        auto_highlight_symbol_runnable_plugins_filter_by_major_mode_condition_and_cycle(),
        auto_highlight_symbol_custom_plugin_macro_registers_command_and_evaluates_properties(),
        auto_highlight_symbol_plugin_property_supports_values_symbols_functions_and_abort(),
        auto_highlight_symbol_invalid_plugin_diagnostics_cover_missing_unregistered_and_unrunnable(
        ),
        auto_highlight_symbol_change_range_updates_state_lighter_and_runs_plugin_init(),
        auto_highlight_symbol_beginning_of_defun_plugin_computes_real_lisp_ranges(),
        auto_highlight_symbol_onekey_macros_install_real_key_commands_and_preserve_custom_map(),
    ]
}
