use expect_test::expect;

use super::ParityBatchCase;

fn ast_grep_descriptor_and_installed_source_inventory_pin_the_exact_release() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_descriptor_and_installed_source_inventory_pin_the_exact_release",
        r##"(let* ((descriptor (cadr (assq 'ast-grep package-alist)))
               (directory (package-desc-dir descriptor))
               (sources
                (sort
                 (directory-files directory t "\\.el\\'")
                 #'string<)))
          (list
           (list
            (package-desc-name descriptor)
            (package-version-join (package-desc-version descriptor))
            (package-desc-summary descriptor)
            (package-desc-reqs descriptor)
            (package-desc-extras descriptor))
           (mapcar
            (lambda (file)
              (list
               (file-name-nondirectory file)
               (file-attribute-size (file-attributes file))
               (with-temp-buffer
                 (insert-file-contents-literally file)
                 (secure-hash 'sha256 (current-buffer)))))
            sources)))"##,
        expect![[
            r#"OK ((ast-grep "20260702.238" "Search code using ast-grep with completing-read interface." ((emacs (28 1))) ((:maintainers ("SunskyXH" . "sunskyxh@gmail.com")) (:authors ("SunskyXH" . "sunskyxh@gmail.com")) (:keywords "tools" "matching") (:revdesc . "28bc6e9ac21a") (:commit . "28bc6e9ac21acf1d1ef58b962b6acd670c27e80f") (:url . "https://github.com/sunskyxh/ast-grep.el"))) (("ast-grep-autoloads.el" 5487 "4d36de0d0d168d2d434317ff7ee89653bfdab7a7007ea973392356586d4765a7") ("ast-grep-consult.el" 3257 "e13dac0be3628f38f2a9cff8f7ac930ce88afd0fd0985f462d120ad05ababb22") ("ast-grep-core.el" 16406 "2b3e0767aa457957e698b3ee438678870cf6dc43bb32f146748946e15c184975") ("ast-grep-helm.el" 5212 "19e8e1e653294fe68dfe587ec9b3311964965c1b5a8f84a0eb6d39e070279ac0") ("ast-grep-ivy.el" 5886 "3444f65db49d4fee26637cab453122c15458c4de6fd7851e800df7821da707fd") ("ast-grep-outline.el" 12714 "e9c9fab478e3057239cf79fd3a191ad1c3441c002cb32956b7c6d30d827f6227") ("ast-grep-pkg.el" 444 "7075b14142d256e96769c7317f13e604b00582713e512b7ffabf082dae9477b3") ("ast-grep-sync.el" 1128 "9bec96cbb53a1d66ce6da0ea6244a2d041c9fc0c567e485b8204e89f0a3448e8") ("ast-grep.el" 9012 "83cc91af93ef27190f3f97d47b8a364470bd836d2b684e844e49551753455051")))"#
        ]],
    )
}

fn ast_grep_main_core_sync_and_outline_callable_surface_is_complete() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_main_core_sync_and_outline_callable_surface_is_complete",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (commandp symbol)
             (macrop symbol)
             (help-function-arglist symbol t)
             (car-safe (interactive-form symbol))))
          '(ast-grep--executable-available-p
            ast-grep--build-command
            ast-grep--command-string
            ast-grep--read-file
            ast-grep--call
            ast-grep--run-command
            ast-grep--reset-candidate-table
            ast-grep--match-from-json
            ast-grep--candidate-display-text
            ast-grep--nerd-icons-available-p
            ast-grep--candidate-icon-prefix
            ast-grep--affixation
            ast-grep--completion-table
            ast-grep--format-candidate
            ast-grep--legacy-candidate-match
            ast-grep--candidate-match
            ast-grep--goto-line-column
            ast-grep--parse-stream-line
            ast-grep--parse-stream-output
            ast-grep--parse-rewrite-line
            ast-grep--collect-rewrites
            ast-grep--match-region
            ast-grep--rewrite-sort
            ast-grep--apply-rewrites
            ast-grep--goto-match
            ast-grep--search-sync
            ast-grep--outline-group-title
            ast-grep--build-outline-command
            ast-grep--run-outline
            ast-grep--outline-parse
            ast-grep--outline-item-position
            ast-grep--outline-flatten
            ast-grep--outline-dedupe-names
            ast-grep--outline-group
            ast-grep--outline-imenu-index
            ast-grep--outline-clear-helm-imenu-cache
            ast-grep--outline-invalidate-imenu-caches
            ast-grep-outline-mode
            ast-grep-outline
            ast-grep--consult-backend-available-p
            ast-grep--ivy-backend-available-p
            ast-grep--helm-backend-available-p
            ast-grep--project-root
            ast-grep--select-backend
            ast-grep--run-search-backend
            ast-grep--backend-description
            ast-grep-describe-backend
            ast-grep-search
            ast-grep-project
            ast-grep-directory
            ast-grep-rewrite
            ast-grep-rewrite-project
            ast-grep-mode))"##,
        expect![
            "OK ((ast-grep--executable-available-p t nil nil nil nil) (ast-grep--build-command t nil nil (pattern &optional directory rewrite) nil) (ast-grep--command-string t nil nil (command) nil) (ast-grep--read-file t nil nil (file) nil) (ast-grep--call t nil nil (command &optional directory label) nil) (ast-grep--run-command t nil nil (pattern &optional directory) nil) (ast-grep--reset-candidate-table t nil nil nil nil) (ast-grep--match-from-json t nil nil (result) nil) (ast-grep--candidate-display-text t nil nil (text) nil) (ast-grep--nerd-icons-available-p t nil nil nil nil) (ast-grep--candidate-icon-prefix t nil nil (candidate) nil) (ast-grep--affixation t nil nil (candidates) nil) (ast-grep--completion-table t nil nil (candidates) nil) (ast-grep--format-candidate t nil nil (match) nil) (ast-grep--legacy-candidate-match t nil nil (candidate) nil) (ast-grep--candidate-match t nil nil (candidate) nil) (ast-grep--goto-line-column t nil nil (line column) nil) (ast-grep--parse-stream-line t nil nil (line) nil) (ast-grep--parse-stream-output t nil nil (output) nil) (ast-grep--parse-rewrite-line t nil nil (line) nil) (ast-grep--collect-rewrites t nil nil (pattern rewrite directory) nil) (ast-grep--match-region t nil nil (match) nil) (ast-grep--rewrite-sort t nil nil (matches) nil) (ast-grep--apply-rewrites t nil nil (matches) nil) (ast-grep--goto-match t nil nil (candidate) nil) (ast-grep--search-sync t nil nil (directory) nil) (ast-grep--outline-group-title t nil nil (type) nil) (ast-grep--build-outline-command t nil nil (file) nil) (ast-grep--run-outline t nil nil (file) nil) (ast-grep--outline-parse t nil nil (output) nil) (ast-grep--outline-item-position t nil nil (item) nil) (ast-grep--outline-flatten t nil nil (items prefix) nil) (ast-grep--outline-dedupe-names t nil nil (leaves) nil) (ast-grep--outline-group t nil nil (entries) nil) (ast-grep--outline-imenu-index t nil nil nil nil) (ast-grep--outline-clear-helm-imenu-cache t nil nil nil nil) (ast-grep--outline-invalidate-imenu-caches t nil nil nil nil) (ast-grep-outline-mode t t nil (&optional arg) interactive) (ast-grep-outline t t nil nil interactive) (ast-grep--consult-backend-available-p t nil nil nil nil) (ast-grep--ivy-backend-available-p t nil nil nil nil) (ast-grep--helm-backend-available-p t nil nil nil nil) (ast-grep--project-root t nil nil nil nil) (ast-grep--select-backend t nil nil nil nil) (ast-grep--run-search-backend t nil nil (backend directory) nil) (ast-grep--backend-description t nil nil nil nil) (ast-grep-describe-backend t t nil nil interactive) (ast-grep-search t t nil (&optional directory) interactive) (ast-grep-project t t nil nil interactive) (ast-grep-directory t t nil (directory) interactive) (ast-grep-rewrite t t nil (&optional directory) interactive) (ast-grep-rewrite-project t t nil nil interactive) (ast-grep-mode t t nil (&optional arg) interactive))"
        ],
    )
}

fn ast_grep_all_declared_variables_have_exact_defaults_and_custom_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_all_declared_variables_have_exact_defaults_and_custom_contracts",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (boundp symbol)
             (and (boundp symbol)
                  (let ((value (symbol-value symbol)))
                    (cond
                     ((hash-table-p value)
                      (list :hash-table
                            (hash-table-test value)
                            (hash-table-count value)))
                     ((keymapp value) :keymap)
                     (t value))))
             (get symbol 'custom-type)
             (get symbol 'custom-group)
             (local-variable-if-set-p symbol)))
          '(ast-grep-executable
            ast-grep-debug
            ast-grep-async-min-input
            ast-grep-use-nerd-icons
            ast-grep-history
            ast-grep-rewrite-history
            ast-grep--candidate-table
            ast-grep--match-property
            ast-grep--legacy-candidate-regexp
            ast-grep--nerd-icons-available-cache
            ast-grep--outline-type-titles
            ast-grep--outline-saved-imenu-function
            ivy-mode
            helm-mode
            ast-grep-search-backend
            ast-grep-outline-mode
            ast-grep-mode))"##,
        expect![[
            r#"OK ((ast-grep-executable t "ast-grep" string nil nil) (ast-grep-debug t nil boolean nil nil) (ast-grep-async-min-input t 3 integer nil nil) (ast-grep-use-nerd-icons t t boolean nil nil) (ast-grep-history t nil nil nil nil) (ast-grep-rewrite-history t nil nil nil nil) (ast-grep--candidate-table t (:hash-table equal 0) nil nil nil) (ast-grep--match-property t ast-grep-match nil nil nil) (ast-grep--legacy-candidate-regexp t "\\`\\(.*\\):\\([0-9]+\\):\\([0-9]+\\):" nil nil nil) (ast-grep--nerd-icons-available-cache t nil nil nil nil) (ast-grep--outline-type-titles t (("class" . "Classes") ("interface" . "Interfaces") ("struct" . "Structs") ("enum" . "Enums") ("trait" . "Traits") ("object" . "Objects") ("module" . "Modules") ("namespace" . "Namespaces") ("function" . "Functions") ("method" . "Methods") ("constructor" . "Constructors") ("field" . "Fields") ("property" . "Properties") ("constant" . "Constants") ("variable" . "Variables") ("type" . "Types") ("macro" . "Macros")) nil nil nil) (ast-grep--outline-saved-imenu-function t unset nil nil t) (ivy-mode t nil nil nil nil) (helm-mode t nil nil nil nil) (ast-grep-search-backend t auto (choice (const :tag "Auto-detect" auto) (const :tag "Consult async" consult) (const :tag "Counsel/ivy async" ivy) (const :tag "Helm async" helm) (const :tag "Sync completing-read" sync)) nil nil) (ast-grep-outline-mode t nil nil nil t) (ast-grep-mode t nil nil nil t))"#
        ]],
    )
    .fresh_process()
}

fn ast_grep_consult_callable_surface_loads_without_optional_consult_dependency() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_consult_callable_surface_loads_without_optional_consult_dependency",
        r##"(list
          (featurep 'ast-grep-consult)
          (featurep 'consult)
          (mapcar
           (lambda (symbol)
             (list symbol
                   (fboundp symbol)
                   (help-function-arglist symbol t)
                   (commandp symbol)))
           '(ast-grep--consult-available-p
             ast-grep--state
             ast-grep--async-builder
             ast-grep--async-source
             ast-grep--search-consult)))"##,
        expect![
            "OK (t nil ((ast-grep--consult-available-p t nil nil) (ast-grep--state t nil nil) (ast-grep--async-builder t (input directory) nil) (ast-grep--async-source t (directory) nil) (ast-grep--search-consult t (directory) nil)))"
        ],
    )
}

fn ast_grep_ivy_callable_and_variable_surface_loads_without_optional_dependencies()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_ivy_callable_and_variable_surface_loads_without_optional_dependencies",
        r##"(list
          (featurep 'ast-grep-ivy)
          (featurep 'ivy)
          (featurep 'counsel)
          counsel--async-timer
          ast-grep--ivy-process-name
          ast-grep--ivy-generation
          (mapcar
           (lambda (symbol)
             (list symbol
                   (fboundp symbol)
                   (help-function-arglist symbol t)
                   (commandp symbol)))
           '(ast-grep--ivy-available-p
             ast-grep--command-shell-string
             ast-grep--ivy-more-chars
             ast-grep--ivy-next-generation
             ast-grep--ivy-cancel-pending-command
             ast-grep--ivy-stop-process
             ast-grep--ivy-current-process-p
             ast-grep--ivy-async-filter
             ast-grep--ivy-collection
             ast-grep--ivy-action
             ast-grep--ivy-display-transformer
             ast-grep--search-ivy)))"##,
        expect![[
            r#"OK (t nil nil nil " *counsel*" 0 ((ast-grep--ivy-available-p t nil nil) (ast-grep--command-shell-string t (command) nil) (ast-grep--ivy-more-chars t (input) nil) (ast-grep--ivy-next-generation t nil nil) (ast-grep--ivy-cancel-pending-command t nil nil) (ast-grep--ivy-stop-process t nil nil) (ast-grep--ivy-current-process-p t (process generation) nil) (ast-grep--ivy-async-filter t (process raw &optional generation) nil) (ast-grep--ivy-collection t (directory) nil) (ast-grep--ivy-action t (candidate) nil) (ast-grep--ivy-display-transformer t (candidate) nil) (ast-grep--search-ivy t (directory) nil)))"#
        ]],
    )
}

fn ast_grep_helm_callable_and_variable_surface_loads_without_optional_dependency() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_helm_callable_and_variable_surface_loads_without_optional_dependency",
        r##"(list
          (featurep 'ast-grep-helm)
          (featurep 'helm)
          (boundp 'helm-pattern)
          ast-grep--helm-preview-buffers
          (mapcar
           (lambda (symbol)
             (list symbol
                   (fboundp symbol)
                   (help-function-arglist symbol t)
                   (commandp symbol)))
           '(ast-grep--helm-ensure-function
             ast-grep--helm-available-p
             ast-grep--helm-command
             ast-grep--helm-candidates-process
             ast-grep--helm-display-candidate
             ast-grep--helm-filter-one-by-one
             ast-grep--helm-action
             ast-grep--helm-preview
             ast-grep--helm-cleanup
             ast-grep--helm-source
             ast-grep--search-helm)))"##,
        expect![
            "OK (t nil nil nil ((ast-grep--helm-ensure-function t (function) nil) (ast-grep--helm-available-p t nil nil) (ast-grep--helm-command t (input directory) nil) (ast-grep--helm-candidates-process t (directory) nil) (ast-grep--helm-display-candidate t (candidate) nil) (ast-grep--helm-filter-one-by-one t (line) nil) (ast-grep--helm-action t (candidate) nil) (ast-grep--helm-preview t (candidate) nil) (ast-grep--helm-cleanup t nil nil) (ast-grep--helm-source t (directory) nil) (ast-grep--search-helm t (directory) nil)))"
        ],
    )
}

pub(super) fn registry_ast_grep_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ast_grep_descriptor_and_installed_source_inventory_pin_the_exact_release(),
        ast_grep_main_core_sync_and_outline_callable_surface_is_complete(),
        ast_grep_all_declared_variables_have_exact_defaults_and_custom_contracts(),
    ]
}

pub(super) fn registry_ast_grep_consult_batch_cases() -> Vec<ParityBatchCase> {
    vec![ast_grep_consult_callable_surface_loads_without_optional_consult_dependency()]
}

pub(super) fn registry_ast_grep_ivy_batch_cases() -> Vec<ParityBatchCase> {
    vec![ast_grep_ivy_callable_and_variable_surface_loads_without_optional_dependencies()]
}

pub(super) fn registry_ast_grep_helm_batch_cases() -> Vec<ParityBatchCase> {
    vec![ast_grep_helm_callable_and_variable_surface_loads_without_optional_dependency()]
}
