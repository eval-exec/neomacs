use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_exact_package_descriptor_dependency_and_provenance_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exact_package_descriptor_dependency_and_provenance_match",
        r##"(let* ((description
                                 (cadr
                                  (assq
                                   'auto-complete
                                   package-alist)))
                                (popup-description
                                 (cadr
                                  (assq
                                   'popup
                                   package-alist))))
                           (list
                            (package-desc-name description)
                            (package-version-join
                             (package-desc-version description))
                            (package-desc-reqs description)
                            (package-desc-summary description)
                            (package-desc-kind description)
                            (package-desc-extras description)
                            (package-desc-name popup-description)
                            (package-version-join
                             (package-desc-version popup-description))
                            (package-desc-reqs popup-description)
                            ac-version
                            ac-version-major
                            ac-version-minor
                            popup-version
                            (featurep 'auto-complete)
                            (featurep 'popup)))"##,
        expect![[
            r#"OK (auto-complete "20251231.1622" ((emacs (25 1)) (popup (0 5 8))) "Auto Completion for GNU Emacs." nil ((:maintainers ("Jen-Chieh Shen" . "jcs090218@gmail.com")) (:authors ("Tomohiro Matsuyama" . "m2ym.pub@gmail.com")) (:keywords "completion" "convenience") (:revdesc . "07f9915e0834") (:commit . "07f9915e08342410b933145d7934998709753a29") (:url . "https://github.com/auto-complete/auto-complete")) popup "20251231.1622" ((emacs (24 3))) "1.5.1" 1 5 "0.5.9" t t)"#
        ]],
    )
}

fn auto_complete_installed_payload_matches_exact_archive_sources_and_dictionary_inventory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_installed_payload_matches_exact_archive_sources_and_dictionary_inventory",
        r##"(let* ((directory
                                 (file-name-directory
                                  (locate-library
                                   "auto-complete")))
                                (dict-directory
                                 (expand-file-name
                                  "dict"
                                  directory))
                                (source-files
                                 '("auto-complete.el"
                                   "auto-complete-config.el"
                                   "auto-complete-pkg.el"))
                                (source-data
                                 (mapcar
                                  (lambda (name)
                                    (let ((file
                                           (expand-file-name
                                            name
                                            directory)))
                                      (list
                                       name
                                       (file-attribute-size
                                        (file-attributes file))
                                       (with-temp-buffer
                                         (set-buffer-multibyte nil)
                                         (insert-file-contents-literally
                                          file)
                                         (secure-hash
                                          'sha256
                                          (current-buffer))))))
                                  source-files))
                                (dict-files
                                 (sort
                                  (directory-files
                                   dict-directory
                                   nil
                                   "\\`[^.]")
                                  #'string<))
                                (dict-data
                                 (mapcar
                                  (lambda (name)
                                    (let ((file
                                           (expand-file-name
                                            name
                                            dict-directory)))
                                      (with-temp-buffer
                                        (insert-file-contents-literally
                                         file)
                                        (list
                                         name
                                         (file-attribute-size
                                          (file-attributes file))
                                         (count-lines
                                          (point-min)
                                          (point-max))
                                         (buffer-substring-no-properties
                                          (point-min)
                                          (line-end-position))
                                         (progn
                                           (goto-char (point-max))
                                           (forward-line -1)
                                           (buffer-substring-no-properties
                                            (line-beginning-position)
                                            (line-end-position)))))))
                                  dict-files)))
                           (list
                            source-data
                            (length dict-files)
                            (car dict-data)
                            (nth 2 dict-data)
                            (assoc "php-mode" dict-data)
                            (assoc "python-mode" dict-data)
                            (car (last dict-data))))"##,
        expect![[
            r#"OK ((("auto-complete.el" 78146 "3ffab0554d2f6f1c8a278fe4a2f163a6f72568062724af53af90506e5c6bc589") ("auto-complete-config.el" 21014 "b1aa1772fe70de065ae60666b0fb6db7f852d67e907bee1aa1c69fe7e74abec1") ("auto-complete-pkg.el" 474 "954b1ed239a7d0ee2c1a63bea9243af681d81364b137c045497bcef3023b66f2")) 32 ("ada-mode" 448 72 "abort" "xor") ("c-mode" 385 55 "auto" "while") ("php-mode" 108127 6144 "abs" "insteadof") ("python-mode" 3023 379 "ArithmeticError" "zlib") ("verilog-mode" 2366 313 "`define" "zi_zd"))"#
        ]],
    )
}

fn auto_complete_builtin_source_definitions_and_generated_commands_match_exactly() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_builtin_source_definitions_and_generated_commands_match_exactly",
        r##"(mapcar
                          (lambda (pair)
                            (let ((source (car pair))
                                  (command (cdr pair)))
                              (list
                               source
                               (symbol-value source)
                               command
                               (interactive-form command))))
                          '((ac-source-words-in-buffer
                             . ac-complete-words-in-buffer)
                            (ac-source-words-in-all-buffer
                             . ac-complete-words-in-all-buffer)
                            (ac-source-words-in-same-mode-buffers
                             . ac-complete-words-in-same-mode-buffers)
                            (ac-source-symbols
                             . ac-complete-symbols)
                            (ac-source-functions
                             . ac-complete-functions)
                            (ac-source-variables
                             . ac-complete-variables)
                            (ac-source-features
                             . ac-complete-features)
                            (ac-source-abbrev
                             . ac-complete-abbrev)
                            (ac-source-files-in-current-dir
                             . ac-complete-files-in-current-dir)
                            (ac-source-filename
                             . ac-complete-filename)
                            (ac-source-dictionary
                             . ac-complete-dictionary)))"##,
        expect![[
            r#"OK ((ac-source-words-in-buffer ((candidates . ac-word-candidates)) ac-complete-words-in-buffer (interactive nil)) (ac-source-words-in-all-buffer ((init . ac-update-word-index) (candidates . ac-word-candidates)) ac-complete-words-in-all-buffer (interactive nil)) (ac-source-words-in-same-mode-buffers ((init . ac-update-word-index) (candidates ac-word-candidates (lambda (buffer) (derived-mode-p (buffer-local-value 'major-mode buffer))))) ac-complete-words-in-same-mode-buffers (interactive nil)) (ac-source-symbols ((candidates . ac-symbol-candidates) (document . ac-symbol-documentation) (symbol . "s") (cache)) ac-complete-symbols (interactive nil)) (ac-source-functions ((candidates . ac-function-candidates) (document . ac-symbol-documentation) (symbol . "f") (prefix . "(\\(\\(?:\\sw\\|\\s_\\)+\\)") (cache)) ac-complete-functions (interactive nil)) (ac-source-variables ((candidates . ac-variable-candidates) (document . ac-symbol-documentation) (symbol . "v") (cache)) ac-complete-variables (interactive nil)) (ac-source-features ((depends find-func) (candidates . ac-emacs-lisp-feature-candidates) (prefix . "require +'\\(\\(?:\\sw\\|\\s_\\)*\\)") (requires . 0)) ac-complete-features (interactive nil)) (ac-source-abbrev ((candidates mapcar 'popup-x-to-string (append (vconcat local-abbrev-table global-abbrev-table) nil)) (action . expand-abbrev) (symbol . "a") (cache)) ac-complete-abbrev (interactive nil)) (ac-source-files-in-current-dir ((candidates directory-files default-directory) (cache)) ac-complete-files-in-current-dir (interactive nil)) (ac-source-filename ((init setq ac-filename-cache nil) (candidates . ac-filename-candidate) (prefix . valid-file) (requires . 0) (action . ac-start) (limit)) ac-complete-filename (interactive nil)) (ac-source-dictionary ((candidates . ac-buffer-dictionary) (symbol . "d")) ac-complete-dictionary (interactive nil)))"#
        ]],
    )
}

fn auto_complete_custom_defaults_types_standard_values_and_aliases_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_custom_defaults_types_standard_values_and_aliases_match",
        r##"(list
                          (mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (symbol-value symbol)
                              (get symbol 'custom-type)
                              (get symbol 'standard-value)
                              (get symbol 'custom-group)
                              (documentation-property
                               symbol
                               'variable-documentation
                               t)))
                           '(ac-delay
                             ac-auto-show-menu
                             ac-show-menu-immediately-on-auto-complete
                             ac-expand-on-auto-complete
                             ac-use-comphist
                             ac-comphist-threshold
                             ac-use-quick-help
                             ac-menu-height
                             ac-candidate-limit
                             ac-auto-start
                             ac-ignore-case
                             ac-dwim
                             ac-candidate-menu-min
                             ac-max-width))
                          (mapcar
                           (lambda (alias)
                             (list
                              alias
                              (indirect-variable alias)
                              (symbol-value alias)))
                           '(ac-user-dictionary-files
                             ac-candidate-menu-height
                             ac-candidate-max
                             ac-quick-help-prefer-x
                             ac-ignores
                             ac-target
                             ac-complete-mode-map
                             ac-source-emacs-lisp-features)))"##,
        expect![[
            r#"OK (((ac-delay 0.1 float (0.1) nil "Delay to completions will be available.") (ac-auto-show-menu 0.8 (choice (const :tag "Yes" t) (const :tag "Never" nil) (float :tag "Timer")) (0.8) nil "Non-nil means completion menu will be automatically shown.") (ac-show-menu-immediately-on-auto-complete t boolean (t) nil "Non-nil means menu will be showed immediately on `auto-complete'.") (ac-expand-on-auto-complete t boolean (t) nil "Non-nil means expand whole common part on first time `auto-complete'.") (ac-use-comphist t boolean (t) nil "Non-nil means use intelligent completion history.") (ac-comphist-threshold 0.7 float (0.7) nil "Percentage of ignoring low scored candidates.") (ac-use-quick-help t boolean (t) nil "Non-nil means use quick help.") (ac-menu-height 10 integer (10) nil "Max height of candidate menu.") (ac-candidate-limit nil integer (nil) nil "Limit number of candidates.  Non-integer means no limit.") (ac-auto-start 2 (choice (const :tag "Yes" t) (const :tag "Never" nil) (integer :tag "Require")) (2) nil "Non-nil means completion will be started automatically.\nPositive integer means if a length of a word you entered is larger than\nthe value, completion will be started automatically.\nIf you specify nil, never be started automatically.") (ac-ignore-case smart (choice (const :tag "Yes" t) (const :tag "Smart" smart) (const :tag "No" nil)) ('smart) nil "Non-nil means `auto-complete' ignores case.\nIf this value is `smart', `auto-complete' ignores case only when\na prefix doesn't contain any upper case letters.") (ac-dwim t boolean (t) nil "Non-nil means `auto-complete' works based on Do What I Mean.") (ac-candidate-menu-min 1 integer (1) nil "Number of candidates required to display menu.") (ac-max-width nil (choice (const :tag "No limit" nil) (const :tag "Character Limit" 25) (const :tag "Window Ratio Limit" 0.5)) (nil) nil "Maximum width for `auto-complete' menu to have.")) ((ac-user-dictionary-files ac-dictionary-files ("~/.dict")) (ac-candidate-menu-height ac-menu-height 10) (ac-candidate-max ac-candidate-limit nil) (ac-quick-help-prefer-x ac-quick-help-prefer-pos-tip t) (ac-ignores ac-stop-words nil) (ac-target ac-prefix nil) (ac-complete-mode-map ac-completing-map (keymap (prior . ac-previous-page) (next . ac-next-page) (C-up . ac-quick-help-scroll-up) (C-down . ac-quick-help-scroll-down) (67108927 . ac-help) (M-f1 . ac-persist-help) (f1 . ac-help) (up . ac-previous) (down . ac-next) (27 keymap (57 . ac-complete-select-9) (56 . ac-complete-select-8) (55 . ac-complete-select-7) (54 . ac-complete-select-6) (53 . ac-complete-select-5) (52 . ac-complete-select-4) (51 . ac-complete-select-3) (50 . ac-complete-select-2) (49 . ac-complete-select-1) (16 . ac-quick-help-scroll-up) (14 . ac-quick-help-scroll-down) (67108927 . ac-persist-help) (112 . ac-previous) (110 . ac-next) (9 . auto-complete)) (13 . ac-complete) (tab . ac-expand) (9 . ac-expand))) (ac-source-emacs-lisp-features ac-source-features ((depends find-func) (candidates . ac-emacs-lisp-feature-candidates) (prefix . "require +'\\(\\(?:\\sw\\|\\s_\\)*\\)") (requires . 0)))))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_command_maps_expose_exact_editing_navigation_and_help_bindings() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_command_maps_expose_exact_editing_navigation_and_help_bindings",
        r##"(mapcar
                          (lambda (map)
                            (list
                             (car map)
                             (mapcar
                              (lambda (key)
                                (list
                                 key
                                 (lookup-key
                                  (symbol-value (car map))
                                  (kbd key))))
                              (cdr map))))
                          '((ac-mode-map
                             "M-TAB")
                            (ac-completing-map
                             "TAB"
                             "<tab>"
                             "RET"
                             "M-TAB"
                             "M-n"
                             "M-p"
                             "<down>"
                             "<up>"
                             "<f1>"
                             "M-<f1>"
                             "<next>"
                             "<prior>"
                             "M-1"
                             "M-9")
                            (ac-menu-map
                             "RET"
                             "C-n"
                             "C-p"
                             "C-s"
                             "<mouse-1>")))"##,
        expect![[
            r#"OK ((ac-mode-map (("M-TAB" nil))) (ac-completing-map (("TAB" ac-expand) ("<tab>" ac-expand) ("RET" ac-complete) ("M-TAB" auto-complete) ("M-n" ac-next) ("M-p" ac-previous) ("<down>" ac-next) ("<up>" ac-previous) ("<f1>" ac-help) ("M-<f1>" ac-persist-help) ("<next>" ac-next-page) ("<prior>" ac-previous-page) ("M-1" ac-complete-select-1) ("M-9" ac-complete-select-9))) (ac-menu-map (("RET" ac-complete) ("C-n" ac-next) ("C-p" ac-previous) ("C-s" ac-isearch) ("<mouse-1>" ac-mouse-1))))"#
        ]],
    )
}

fn auto_complete_autoloads_preserve_commands_modes_and_customization_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_autoloads_preserve_commands_modes_and_customization_contract",
        r##"(list
                          (symbol-function 'auto-complete)
                          (symbol-function 'auto-complete-mode)
                          (symbol-function 'global-auto-complete-mode)
                          (symbol-function 'ac-config-default)
                          (custom-autoload
                           'global-auto-complete-mode
                           "auto-complete"
                           nil)
                          (get 'auto-complete-mode 'custom-type)
                          (get 'global-auto-complete-mode 'custom-type)
                          (get 'auto-complete-mode 'variable-documentation)
                          (get 'global-auto-complete-mode
                               'variable-documentation))"##,
        expect![[
            r#"OK ((autoload "auto-complete" "Start auto-completion at current point.\n\n(fn &optional SOURCES)" t nil) (autoload "auto-complete" "AutoComplete mode\n\nThis is a minor mode.  If called interactively, toggle the\n`Auto-Complete mode' mode.  If the prefix argument is positive, enable\nthe mode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable `auto-complete-mode'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled.\n\n(fn &optional ARG)" t nil) (autoload "auto-complete" "Toggle Auto-Complete mode in many buffers.\nSpecifically, Auto-Complete mode is enabled in all buffers where\n`auto-complete-mode-maybe' would do it.\n\nWith prefix ARG, enable Global Auto-Complete mode if ARG is positive;\notherwise, disable it.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.\nEnable the mode if ARG is nil, omitted, or is a positive number.\nDisable the mode if ARG is a negative number.\n\nSee `auto-complete-mode' for more information on Auto-Complete mode.\n\n(fn &optional ARG)" t nil) (autoload "auto-complete-config" "No documentation." nil nil) nil nil nil nil "Non-nil if Global Auto-Complete mode is enabled.\nSee the `global-auto-complete-mode' command\nfor a description of this minor mode.\nSetting this variable directly does not take effect;\neither customize it (see the info node `Easy Customization')\nor call the function `global-auto-complete-mode'.")"#
        ]],
    )
}

pub(super) fn registry_auto_complete_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_exact_package_descriptor_dependency_and_provenance_match(),
        auto_complete_installed_payload_matches_exact_archive_sources_and_dictionary_inventory(),
        auto_complete_builtin_source_definitions_and_generated_commands_match_exactly(),
        auto_complete_custom_defaults_types_standard_values_and_aliases_match(),
        auto_complete_command_maps_expose_exact_editing_navigation_and_help_bindings(),
    ]
}

pub(super) fn registry_auto_complete_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_autoloads_preserve_commands_modes_and_customization_contract()]
}
