use expect_test::expect;

use super::ParityBatchCase;

fn aurora_config_mode_descriptor_and_installed_payload_match_exact_melpa_archive() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aurora_config_mode_descriptor_and_installed_payload_match_exact_melpa_archive",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'aurora-config-mode
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (files
                (mapcar
                 (lambda (name)
                   (expand-file-name
                    name
                    directory))
                 '("aurora-config-mode-pkg.el"
                   "aurora-config-mode.el"))))
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
                 (secure-hash
                  'sha256
                  (current-buffer)))))
            files)))"##,
        expect![[
            r#"OK ((aurora-config-mode "20180216.2302" "Major mode for Apache Aurora configuration files." nil ((:maintainers ("Berk D. Demir" . "bdd@mindcast.org")) (:authors ("Berk D. Demir" . "bdd@mindcast.org")) (:keywords "languages" "configuration") (:revdesc . "8273ec7937a2") (:commit . "8273ec7937a21b469b9dbb6c11714255b890f410") (:url . "https://github.com/bdd/aurora-config.el"))) (("aurora-config-mode-pkg.el" 446 "969bb7486c9a10ca6210f6649555b8ae7ba8a4e7e1b382500b4f961ee7ae88bf") ("aurora-config-mode.el" 3705 "63e5a15768e7ef53e499508e166566c42c1fa867b86ea1733adc7b362bbb3492")))"#
        ]],
    )
}

fn aurora_config_mode_complete_prefixed_symbol_inventory_records_every_surface() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aurora_config_mode_complete_prefixed_symbol_inventory_records_every_surface",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (string-prefix-p
                     "aurora-config"
                     name)
                    (not
                     (string-prefix-p
                      "aurora-config-test-"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (boundp symbol)
                   (and
                    (custom-variable-p symbol)
                    t)
                   (and
                    (macrop symbol)
                    t)
                   (when
                       (fboundp symbol)
                     (copy-tree
                      (help-function-arglist
                       symbol
                       t))))
                  symbols)))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name
               (car left))
              (symbol-name
               (car right))))))"##,
        expect![
            "OK ((aurora-config-aurora-struct-keywords nil t nil nil nil) (aurora-config-diff t nil nil nil (jobpath)) (aurora-config-font-lock-keywords nil t nil nil nil) (aurora-config-inspect t nil nil nil (jobpath)) (aurora-config-last-job-path nil t nil nil nil) (aurora-config-mode t nil nil nil nil) (aurora-config-mode-abbrev-table nil t nil nil nil) (aurora-config-mode-autoloads nil nil nil nil nil) (aurora-config-mode-hook nil t nil nil nil) (aurora-config-mode-map nil t nil nil nil) (aurora-config-mode-syntax-table nil t nil nil nil) (aurora-config-pystachio-struct-keywords nil t nil nil nil) (aurora-config-read-jobpath t nil nil nil nil) (aurora-config-run-aurora t nil nil nil (command jobpath)))"
        ],
    )
}

fn aurora_config_mode_all_callable_metadata_interactive_forms_docs_and_sources_are_exact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_all_callable_metadata_interactive_forms_docs_and_sources_are_exact",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (commandp symbol)
             (interactive-form symbol)
             (copy-tree
              (help-function-arglist
               symbol
               t))
             (documentation symbol t)
             (when-let
                 ((source
                   (symbol-file symbol 'defun)))
               (file-name-nondirectory
                source))))
          '(aurora-config-read-jobpath
            aurora-config-run-aurora
            aurora-config-inspect
            aurora-config-diff
            aurora-config-mode))"##,
        expect![[
            r#"OK ((aurora-config-read-jobpath t nil nil nil "Read job path from minibuffer with history defaulting to buffer local last used." "aurora-config-mode.el") (aurora-config-run-aurora t nil nil (command jobpath) "Run `aurora COMMAND JOBPATH' with the config in current buffer." "aurora-config-mode.el") (aurora-config-inspect t t (interactive (list (aurora-config-read-jobpath))) (jobpath) "Run `aurora inspect JOBPATH' with the config in current buffer." "aurora-config-mode.el") (aurora-config-diff t t (interactive (list (aurora-config-read-jobpath))) (jobpath) "Run `aurora diff JOBPATH' with the config in current buffer." "aurora-config-mode.el") (aurora-config-mode t t (interactive nil) nil "Major mode for Aurora configuration files, derived from Python mode.\n\nIn addition to any hooks its parent mode `python-mode' might have run,\nthis mode runs the hook `aurora-config-mode-hook', as the final or\npenultimate step during initialization.\n\n\\{aurora-config-mode-map}" "aurora-config-mode.el"))"#
        ]],
    )
}

fn aurora_config_mode_declared_constants_state_and_keymap_have_exact_contracts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aurora_config_mode_declared_constants_state_and_keymap_have_exact_contracts",
        r##"(list
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (boundp symbol)
              (copy-tree
               (symbol-value symbol))
              (default-boundp symbol)
              (default-value symbol)
              (special-variable-p symbol)
              (local-variable-if-set-p symbol)
              (and
               (custom-variable-p symbol)
               t)
              (documentation-property
               symbol
               'variable-documentation
               t)
              (when-let
                  ((source
                    (symbol-file symbol 'defvar)))
                (file-name-nondirectory
                 source))))
           '(aurora-config-aurora-struct-keywords
             aurora-config-pystachio-struct-keywords
             aurora-config-font-lock-keywords
             aurora-config-last-job-path))
          (list
           (keymapp aurora-config-mode-map)
           (lookup-key
            aurora-config-mode-map
            (kbd "C-c a"))
           (lookup-key
            aurora-config-mode-map
            (kbd "C-c a i"))
           (lookup-key
            aurora-config-mode-map
            (kbd "C-c a d"))
           (lookup-key
            aurora-config-mode-map
            (kbd "C-c a x"))
           (documentation-property
            'aurora-config-mode-map
            'variable-documentation
            t)
           (file-name-nondirectory
            (symbol-file
             'aurora-config-mode-map
             'defvar))))"##,
        expect![[
            r#"OK (((aurora-config-aurora-struct-keywords t ("HealthCheckConfig" "Job" "Process" "JVMProcess" "Resources" "SequentialTask" "Service" "Task" "UpdateConfig") t ("HealthCheckConfig" "Job" "Process" "JVMProcess" "Resources" "SequentialTask" "Service" "Task" "UpdateConfig") t nil nil nil "aurora-config-mode.el") (aurora-config-pystachio-struct-keywords t ("Enum" "Integer" "List" "Map" "String" "Struct") t ("Enum" "Integer" "List" "Map" "String" "Struct") t nil nil nil "aurora-config-mode.el") (aurora-config-font-lock-keywords t (("\\_<\\(HealthCheckConfig\\|J\\(?:VMProcess\\|ob\\)\\|Process\\|Resources\\|Se\\(?:quentialTask\\|rvice\\)\\|Task\\|UpdateConfig\\)\\_>" . font-lock-function-name-face) ("\\_<\\(Enum\\|Integer\\|List\\|Map\\|Str\\(?:ing\\|uct\\)\\)\\_>" . font-lock-type-face)) t (("\\_<\\(HealthCheckConfig\\|J\\(?:VMProcess\\|ob\\)\\|Process\\|Resources\\|Se\\(?:quentialTask\\|rvice\\)\\|Task\\|UpdateConfig\\)\\_>" . font-lock-function-name-face) ("\\_<\\(Enum\\|Integer\\|List\\|Map\\|Str\\(?:ing\\|uct\\)\\)\\_>" . font-lock-type-face)) t nil nil nil "aurora-config-mode.el") (aurora-config-last-job-path t "smf1/" t "smf1/" t t nil nil "aurora-config-mode.el")) (t (keymap (100 . aurora-config-diff) (105 . aurora-config-inspect)) aurora-config-inspect aurora-config-diff nil "`aurora-config-mode' key map." "aurora-config-mode.el"))"#
        ]],
    )
    .fresh_process()
}

fn aurora_config_mode_source_reloads_preserve_defvars_reset_constants_and_deduplicate_file_rules()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_source_reloads_preserve_defvars_reset_constants_and_deduplicate_file_rules",
        r##"(let ((source
                (symbol-file
                 'aurora-config-mode
                 'defun)))
          (setq-default
           aurora-config-last-job-path
           "fixture/default")
          (setq
           aurora-config-aurora-struct-keywords
           '(fixture-aurora)
           aurora-config-pystachio-struct-keywords
           '(fixture-pystachio)
           aurora-config-font-lock-keywords
           '(fixture-font-lock))
          (define-key
           aurora-config-mode-map
           (kbd "C-c a x")
           'fixture-command)
          (load source nil t)
          (load source nil t)
          (list
           aurora-config-aurora-struct-keywords
           aurora-config-pystachio-struct-keywords
           aurora-config-font-lock-keywords
           (default-value
            'aurora-config-last-job-path)
           (lookup-key
            aurora-config-mode-map
            (kbd "C-c a x"))
           (mapcar
            (lambda (regexp)
              (let ((count 0))
                (dolist
                    (entry auto-mode-alist count)
                  (when
                      (and
                       (equal
                        (car entry)
                        regexp)
                       (eq
                        (cdr entry)
                        'aurora-config-mode))
                    (setq count
                          (1+ count))))))
            '("\\.aurora\\'"
              "\\.mesos\\'"))))"##,
        expect![[
            r#"OK (("HealthCheckConfig" "Job" "Process" "JVMProcess" "Resources" "SequentialTask" "Service" "Task" "UpdateConfig") ("Enum" "Integer" "List" "Map" "String" "Struct") (("\\_<\\(HealthCheckConfig\\|J\\(?:VMProcess\\|ob\\)\\|Process\\|Resources\\|Se\\(?:quentialTask\\|rvice\\)\\|Task\\|UpdateConfig\\)\\_>" . font-lock-function-name-face) ("\\_<\\(Enum\\|Integer\\|List\\|Map\\|Str\\(?:ing\\|uct\\)\\)\\_>" . font-lock-type-face)) "fixture/default" fixture-command (1 1))"#
        ]],
    )
}

fn aurora_config_mode_generated_autoloads_register_files_commands_prefixes_and_history()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_generated_autoloads_register_files_commands_prefixes_and_history",
        r##"(let* ((history
                (seq-find
                 (lambda (entry)
                   (and
                    (stringp
                     (car entry))
                    (string-suffix-p
                     "aurora-config-mode-autoloads.el"
                     (car entry))))
                 load-history))
               (events
                (seq-filter
                 (lambda (event)
                   (memq
                    (car-safe event)
                    '(defun provide)))
                 (cdr history))))
          (list
           (featurep
            'aurora-config-mode-autoloads)
           (featurep
            'aurora-config-mode)
           events
           (and
            (boundp 'definition-prefixes)
            (gethash
             "aurora-config-mode"
             definition-prefixes))
           (seq-filter
            (lambda (entry)
              (eq
               (cdr entry)
               'aurora-config-mode))
            auto-mode-alist)
           (mapcar
            (lambda (symbol)
              (let ((definition
                     (symbol-function symbol)))
                (list
                 symbol
                 (autoloadp definition)
                 (nth 1 definition)
                 (nth 4 definition)
                 (commandp symbol)
                 (help-function-arglist
                  symbol
                  t))))
            '(aurora-config-inspect
              aurora-config-diff
              aurora-config-mode))
           (mapcar
            (lambda (symbol)
              (list
               symbol
               (fboundp symbol)
               (boundp symbol)))
            '(aurora-config-read-jobpath
              aurora-config-run-aurora
              aurora-config-last-job-path))))"##,
        expect![[
            r#"OK (t nil ((defun . aurora-config-inspect) (defun . aurora-config-diff) (defun . aurora-config-mode) (provide . aurora-config-mode-autoloads)) nil (("\\.mesos\\'" . aurora-config-mode) ("\\.aurora\\'" . aurora-config-mode)) ((aurora-config-inspect t "aurora-config-mode" nil t "[Arg list not available until function definition is loaded.]") (aurora-config-diff t "aurora-config-mode" nil t "[Arg list not available until function definition is loaded.]") (aurora-config-mode t "aurora-config-mode" nil t "[Arg list not available until function definition is loaded.]")) ((aurora-config-read-jobpath nil nil) (aurora-config-run-aurora nil nil) (aurora-config-last-job-path nil nil)))"#
        ]],
    )
}

pub(super) fn registry_aurora_config_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurora_config_mode_descriptor_and_installed_payload_match_exact_melpa_archive(),
        aurora_config_mode_complete_prefixed_symbol_inventory_records_every_surface(),
        aurora_config_mode_all_callable_metadata_interactive_forms_docs_and_sources_are_exact(),
        aurora_config_mode_declared_constants_state_and_keymap_have_exact_contracts(),
        aurora_config_mode_source_reloads_preserve_defvars_reset_constants_and_deduplicate_file_rules(),
    ]
}

pub(super) fn registry_aurora_config_mode_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![aurora_config_mode_generated_autoloads_register_files_commands_prefixes_and_history()]
}
