use expect_test::expect;

use super::ParityBatchCase;

fn auto_compile_descriptor_and_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_descriptor_and_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr (assq 'auto-compile package-alist)))
               (directory (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name name directory))
                 '("auto-compile-pkg.el"
                   "auto-compile.el"))))
         (list
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-reqs descriptor)
           (package-desc-extras descriptor))
          (mapcar
           (lambda (file)
             (list
              (file-name-nondirectory file)
              (file-attribute-size
               (file-attributes file))
              (with-temp-buffer
                (insert-file-contents-literally file)
                (secure-hash 'sha256 (current-buffer)))))
           sources)))"##,
        expect![[
            r#"OK ((auto-compile "20260601.1449" "Automatically compile Emacs Lisp libraries." ((emacs (28 1))) ((:maintainers ("Jonas Bernoulli" . "emacs.auto-compile@jonas.bernoulli.dev")) (:authors ("Jonas Bernoulli" . "emacs.auto-compile@jonas.bernoulli.dev")) (:keywords "compile" "convenience" "lisp") (:revdesc . "4db3a0e497fe") (:commit . "4db3a0e497feecc8b3dbeeefacdf363ae60a6392") (:url . "https://github.com/emacscollective/auto-compile"))) (("auto-compile-pkg.el" 508 "34be4dd27a5ec8ff762d0a289a3d4341737c17216ded0c4b7c7be3270de9397e") ("auto-compile.el" 34973 "8596d57356684a1ceab03ee3c65cbf911b01c4fccda6d9052af866469e55c95f")))"#
        ]],
    )
}

fn auto_compile_feature_aliases_and_definition_origins_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_feature_aliases_and_definition_origins_are_exact",
        r##"(list
         (featurep 'auto-compile)
         (eq (symbol-function 'auto-compile-toggle)
             (symbol-function 'toggle-auto-compile))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          '(auto-compile-mode
            auto-compile-on-save-mode
            auto-compile-on-load-mode
            toggle-auto-compile
            auto-compile-toggle
            auto-compile-byte-compile
            auto-compile-delete-dest
            auto-compile-source-file-p
            auto-compile-on-load
            mode-line-auto-compile-control)))"##,
        expect![[
            r#"OK (t nil ((auto-compile-mode t "auto-compile.el") (auto-compile-on-save-mode t "auto-compile.el") (auto-compile-on-load-mode t "auto-compile.el") (toggle-auto-compile t "auto-compile.el") (auto-compile-toggle t "auto-compile.el") (auto-compile-byte-compile t "auto-compile.el") (auto-compile-delete-dest t "auto-compile.el") (auto-compile-source-file-p t "auto-compile.el") (auto-compile-on-load t "auto-compile.el") (mode-line-auto-compile-control t "auto-compile.el")))"#
        ]],
    )
}

fn auto_compile_public_commands_have_exact_interactive_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_public_commands_have_exact_interactive_contracts",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist symbol t)))
         '(auto-compile-mode
           auto-compile-on-save-mode
           auto-compile-on-load-mode
           toggle-auto-compile
           auto-compile-toggle
           auto-compile-toggle-mark-failed-modified
           auto-compile-display-log
           mode-line-toggle-auto-compile
           auto-compile-mode-line-byte-compile))"##,
        expect![[
            r#"OK ((auto-compile-mode t (interactive #1=(list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) (&optional arg)) (auto-compile-on-save-mode t (interactive #1#) (&optional arg)) (auto-compile-on-load-mode t (interactive #1#) (&optional arg)) (toggle-auto-compile t (interactive #2=(let* ((file (and (eq major-mode 'emacs-lisp-mode) (buffer-file-name))) (action (cond (current-prefix-arg (if (> (prefix-numeric-value current-prefix-arg) 0) 'start 'quit)) (file (if (file-exists-p (byte-compile-dest-file file)) 'quit 'start)) (t (let* ((val (read-char-choice "Toggle automatic compilation (s=tart, q=uit, C-g)? " '(115 113)))) (cond ((eql val 115) (let nil 'start)) ((eql val 113) (let nil 'quit)))))))) (list (read-file-name (concat (capitalize (symbol-name action)) " auto-compiling: ") (and file (file-name-directory file)) nil t (and file (file-name-nondirectory file))) action t))) #3=(file action &optional interactive)) (auto-compile-toggle t (interactive #2#) #3#) (auto-compile-toggle-mark-failed-modified t (interactive nil) nil) (auto-compile-display-log t (interactive nil) nil) (mode-line-toggle-auto-compile t (interactive "e") (event)) (auto-compile-mode-line-byte-compile t (interactive "e") (event)))"#
        ]],
    )
}

fn auto_compile_options_have_exact_defaults_and_custom_types() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_options_have_exact_defaults_and_custom_types",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (default-value symbol)
            (get symbol 'standard-value)
            (get symbol 'custom-type)
            (get symbol 'custom-group)))
         '(auto-compile-visit-failed
           auto-compile-mark-failed-modified
           auto-compile-ding
           auto-compile-native-compile
           auto-compile-check-parens
           auto-compile-inhibit-compile-hook
           auto-compile-verbose
           auto-compile-display-buffer
           auto-compile-mode-line-counter
           auto-compile-use-mode-line
           auto-compile-toggle-recompiles
           auto-compile-predicate-function
           auto-compile-delete-stray-dest
           auto-compile-toggle-deletes-nonlib-dest
           auto-compile-source-recreate-deletes-dest))"##,
        expect![[
            r#"OK ((auto-compile-visit-failed t ((funcall #'#[nil (t) #1=(warning-minimum-level t)])) boolean nil) (auto-compile-mark-failed-modified nil ((funcall #'#[nil (nil) #1#])) boolean nil) (auto-compile-ding t ((funcall #'#[nil (t) #1#])) boolean nil) (auto-compile-native-compile nil ((funcall #'#[nil (nil) #1#])) boolean nil) (auto-compile-check-parens t ((funcall #'#[nil (t) #1#])) boolean nil) (auto-compile-inhibit-compile-hook nil ((funcall #'#[nil (nil) #1#])) hook nil) (auto-compile-verbose nil ((funcall #'#[nil (nil) #1#])) boolean nil) (auto-compile-display-buffer t ((funcall #'#[nil (t) #1#])) boolean nil) (auto-compile-mode-line-counter nil ((funcall #'#[nil (nil) #1#])) boolean nil) (auto-compile-use-mode-line mode-line-remote ((funcall #'#[nil ((car (auto-compile--tree-member 'mode-line-remote (default-value 'mode-line-format)))) #1#])) (choice (const :tag "Don't insert" nil) (const :tag "After mode-line-modified" mode-line-modified) (const :tag "After mode-line-remote" mode-line-remote) (sexp :tag "After construct")) nil) (auto-compile-toggle-recompiles t ((funcall #'#[nil (t) #1#])) boolean nil) (auto-compile-predicate-function auto-compile-source-file-p ((funcall #'#[nil ('auto-compile-source-file-p) #1#])) (choice (const auto-compile-source-file-p) (const elx-library-p) function) nil) (auto-compile-delete-stray-dest t ((funcall #'#[nil (t) #1#])) boolean nil) (auto-compile-toggle-deletes-nonlib-dest nil ((funcall #'#[nil (nil) #1#])) boolean nil) (auto-compile-source-recreate-deletes-dest nil ((funcall #'#[nil (nil) #1#])) boolean nil))"#
        ]],
    )
}

fn auto_compile_source_load_history_records_modes_advices_and_provider() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_source_load_history_records_modes_advices_and_provider",
        r##"(let* ((file (locate-library "auto-compile"))
               (history (cdr (assoc file load-history))))
         (seq-filter
          (lambda (event)
            (or
             (memq (car-safe event)
                   '(defun defvar provide))
             (and
              (eq (car-safe event) 'advice)
              (memq (cdr-safe event)
                    '(load require byte-compile-log-warning
                      save-buffers-kill-emacs
                      save-buffers-kill-terminal)))))
          history))"##,
        expect![
            "OK ((defun . auto-compile--static-if) (defun . auto-compile-mode) (defun . auto-compile-on-save-mode) (defun . auto-compile-mode--set-explicitly) (defun . auto-compile-on-save-mode-enable-in-buffer) (defun . auto-compile-mode--turn-on) (defun . auto-compile--tree-member) (defun . auto-compile-modify-mode-line) (defun . toggle-auto-compile) (defun . auto-compile-toggle) (defun . auto-compile-toggle-mark-failed-modified) (defun . auto-compile-source-file-p) (defun . auto-compile--byte-compile-source-file) (defun . byte-compile-log-warning@auto-compile) (defun . auto-compile-byte-compile) (defun . auto-compile--byte-compile-file) (defun . auto-compile-delete-dest) (defun . auto-compile-handle-compile-error) (defun . auto-compile-ding) (defun . save-buffers-kill-emacs@auto-compile) (defun . save-buffers-kill-terminal@auto-compile) (defun . auto-compile-inhibit-compile-detached-git-head) (defun . mode-line-auto-compile-control) (defun . auto-compile-display-log) (defun . mode-line-toggle-auto-compile) (defun . auto-compile-mode-line-byte-compile) (defun . auto-compile-on-load-mode) (defun . load@auto-compile) (defun . require@auto-compile) (defun . auto-compile-on-load) (defun . auto-compile--locate-library) (defun . auto-compile-use-mode-line-set) (provide . auto-compile))"
        ],
    )
}

fn auto_compile_reload_preserves_user_options_and_installs_each_advice_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_reload_preserves_user_options_and_installs_each_advice_once",
        r##"(let ((source (locate-library "auto-compile")))
         (setq auto-compile-check-parens nil
               auto-compile-display-buffer nil
               auto-compile-toggle-recompiles nil)
         (load source nil t t)
         (load source nil t t)
         (list
          auto-compile-check-parens
          auto-compile-display-buffer
          auto-compile-toggle-recompiles
          (mapcar
           (lambda (pair)
             (and
              (advice-member-p (cdr pair) (car pair))
              t))
           '((load . load@auto-compile)
             (require . require@auto-compile)
             (byte-compile-log-warning
              . byte-compile-log-warning@auto-compile)
             (save-buffers-kill-emacs
              . save-buffers-kill-emacs@auto-compile)
             (save-buffers-kill-terminal
              . save-buffers-kill-terminal@auto-compile)))
          (featurep 'auto-compile)))"##,
        expect!["OK (nil nil nil (t t t t t) t)"],
    )
}

fn auto_compile_group_links_and_mode_variable_metadata_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_group_links_and_mode_variable_metadata_are_exact",
        r##"(list
         (get 'auto-compile 'custom-group)
         (get 'auto-compile 'group-documentation)
         (get 'auto-compile-mode 'custom-type)
         (get 'auto-compile-mode 'variable-documentation)
         (get 'auto-compile-on-save-mode 'custom-type)
         (get 'auto-compile-on-load-mode 'custom-type)
         (get 'mode-line-auto-compile 'risky-local-variable)
         (local-variable-if-set-p 'auto-compile-warnings)
         (local-variable-if-set-p
          'auto-compile-pretend-byte-compiled))"##,
        expect![[
            r#"OK (((auto-compile-on-save-mode custom-variable) (auto-compile-visit-failed custom-variable) (auto-compile-mark-failed-modified custom-variable) (auto-compile-ding custom-variable) (auto-compile-native-compile custom-variable) (auto-compile-check-parens custom-variable) (auto-compile-inhibit-compile-hook custom-variable) (auto-compile-verbose custom-variable) (auto-compile-display-buffer custom-variable) (auto-compile-mode-line-counter custom-variable) (auto-compile-use-mode-line custom-variable) (auto-compile-toggle-recompiles custom-variable) (auto-compile-predicate-function custom-variable) (auto-compile-delete-stray-dest custom-variable) (auto-compile-toggle-deletes-nonlib-dest custom-variable) (auto-compile-source-recreate-deletes-dest custom-variable) (auto-compile-on-load-mode custom-variable)) "Automatically compile Emacs Lisp source libraries." nil "Non-nil if Auto-Compile mode is enabled.\nUse the command `auto-compile-mode' to change this variable." boolean boolean t t t)"#
        ]],
    )
}

fn auto_compile_generated_autoloads_register_all_entry_points_without_loading_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_generated_autoloads_register_all_entry_points_without_loading_source",
        r##"(let* ((file
                 (locate-library "auto-compile-autoloads"))
                (history (cdr (assoc file load-history))))
         (list
          (featurep 'auto-compile-autoloads)
          (featurep 'auto-compile)
          (seq-filter
           (lambda (event)
             (memq (car-safe event) '(defun provide)))
           history)
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (fboundp symbol)
              (autoloadp (symbol-function symbol))
              (commandp symbol)))
           '(auto-compile-mode
             auto-compile-on-save-mode
             toggle-auto-compile
             auto-compile-on-load-mode))))"##,
        expect![
            "OK (t nil ((defun . auto-compile-mode) (defun . auto-compile-on-save-mode) (defun . toggle-auto-compile) (defun . auto-compile-on-load-mode) (provide . auto-compile-autoloads)) ((auto-compile-mode t t t) (auto-compile-on-save-mode t t t) (toggle-auto-compile t t t) (auto-compile-on-load-mode t t t)))"
        ],
    )
}

pub(super) fn registry_auto_compile_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_compile_descriptor_and_sources_pin_exact_melpa_payload(),
        auto_compile_feature_aliases_and_definition_origins_are_exact(),
        auto_compile_public_commands_have_exact_interactive_contracts(),
        auto_compile_options_have_exact_defaults_and_custom_types(),
        auto_compile_source_load_history_records_modes_advices_and_provider(),
        auto_compile_reload_preserves_user_options_and_installs_each_advice_once(),
        auto_compile_group_links_and_mode_variable_metadata_are_exact(),
    ]
}

pub(super) fn registry_auto_compile_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_compile_generated_autoloads_register_all_entry_points_without_loading_source()]
}
