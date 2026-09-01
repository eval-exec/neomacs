use expect_test::expect;

use super::ParityBatchCase;

fn auto_dictionary_descriptor_and_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_descriptor_and_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq 'auto-dictionary
                       package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name name directory))
                 '("auto-dictionary-pkg.el"
                   "auto-dictionary.el"))))
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
           sources)))"##,
        expect![[
            r#"OK ((auto-dictionary "20150410.1610" "Automatic dictionary switcher for flyspell." nil ((:maintainers ("Nikolaj Schumacher" . "bugs*nschumde")) (:authors ("Nikolaj Schumacher" . "bugs*nschumde")) (:keywords "wp") (:revdesc . "b364e08009fe") (:commit . "b364e08009fe0062cf0927d8a0582fad5a12b8e7") (:url . "http://nschum.de/src/emacs/auto-dictionary/"))) (("auto-dictionary-pkg.el" 422 "7d2e84f5d0e137a4008f02603fd178bd2154a300cb851234d7cefa678d2c520b") ("auto-dictionary.el" 46235 "fc414c5be331a2d039119bacde01050cf33b15fce6665a17bde3e36ec9317863")))"#
        ]],
    )
}

fn auto_dictionary_feature_aliases_and_definition_origins_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_feature_aliases_and_definition_origins_are_exact",
        r##"(list
         (featurep 'auto-dictionary)
         (eq
          (indirect-variable
           'switch-language-hook)
          'adict-change-dictionary-hook)
         (eq
          (indirect-function 'adict-mode)
          (indirect-function
           'auto-dictionary-mode))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          '(auto-dictionary-mode
            adict-mode
            adict-guess-dictionary
            adict-change-dictionary
            adict-guess-dictionary-maybe
            adict-evaluate-word
            adict-evaluate-buffer
            adict-conditional-insert
            adict-conditional-update
            adict-guess-word-language
            adict-guess-buffer-language)))"##,
        expect![[
            r#"OK (t nil t ((auto-dictionary-mode t "auto-dictionary.el") (adict-mode t "auto-dictionary.el") (adict-guess-dictionary t "auto-dictionary.el") (adict-change-dictionary t "auto-dictionary.el") (adict-guess-dictionary-maybe t "auto-dictionary.el") (adict-evaluate-word t "auto-dictionary.el") (adict-evaluate-buffer t "auto-dictionary.el") (adict-conditional-insert t "auto-dictionary.el") (adict-conditional-update t "auto-dictionary.el") (adict-guess-word-language t "auto-dictionary.el") (adict-guess-buffer-language t "auto-dictionary.el")))"#
        ]],
    )
}

fn auto_dictionary_public_entry_points_have_exact_interactive_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_public_entry_points_have_exact_interactive_contracts",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist symbol t)))
         '(auto-dictionary-mode
           adict-mode
           adict-guess-dictionary
           adict-change-dictionary
           adict-guess-dictionary-maybe
           adict-conditional-insert
           adict-guess-word-language
           adict-guess-buffer-language))"##,
        expect![
            "OK ((auto-dictionary-mode t (interactive #1=(list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) #2=(&optional arg)) (adict-mode t (interactive #1#) #2#) (adict-guess-dictionary t (interactive nil) (&optional idle-only)) (adict-change-dictionary t (interactive nil) (&optional lang)) (adict-guess-dictionary-maybe nil nil (timer-buffer)) (adict-conditional-insert nil nil (&rest language-text-pairs)) (adict-guess-word-language nil nil (word)) (adict-guess-buffer-language nil nil (&optional idle-only)))"
        ],
    )
}

fn auto_dictionary_options_language_order_and_deterministic_defaults_are_exact() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_options_language_order_and_deterministic_defaults_are_exact",
        r##"(list
         adict-idle-time
         adict-change-threshold
         adict-language-list
         adict-dictionary-list
         adict-stop-updating-on-dictionary-change
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (get symbol 'custom-type)
             (get symbol 'custom-group)))
          '(adict-idle-time
            adict-change-threshold
            adict-change-dictionary-hook
            adict-dictionary-list))
         (get 'adict-conditional-text-face
              'face-defface-spec))"##,
        expect![[
            r#"OK (2 0.02 (nil "en" "de" "fr" "es" "sv" "sl" "hu" "ro" "pt" "nb" "da" "grc" "el" "hi" "nn" "ca" "eo" "sk") (("en" . "en") ("de" . "de") ("fr" . "fr") ("es" . "es") ("sv" . "sv") ("sl" . "sl") ("hu" . "hu") ("ro" . "ro") ("pt" . "pt") ("nb" . "nb") ("da" . "da") ("grc" . "grc") ("el" . "el") ("hi" . "hi") ("nn" . "nn") ("ca" . "ca") ("eo" . "eo") ("sk" . "sk")) t ((adict-idle-time number nil) (adict-change-threshold number nil) (adict-change-dictionary-hook hook nil) (adict-dictionary-list (repeat (cons (choice (const "en") (const "de") (const "fr") (const "es") (const "sv") (const "sl") (const "hu") (const "ro") (const "pt") (const "nb") (const "da") (const "grc") (const "el") (const "hi") (const "nn") (const "ca") (const "eo") (const "sk")) (choice (const :tag "Off" nil) (string :tag "Dictionary name")))) nil)) ((((class color) (background dark)) (:background "MediumBlue")) (((class color) (background light)) (:background "turquoise"))))"#
        ]],
    )
}

fn auto_dictionary_word_table_covers_each_language_and_unknown_bucket() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_word_table_covers_each_language_and_unknown_bucket",
        r##"(list
         (eq
          (hash-table-test adict-hash)
          'equal)
         (= (hash-table-count adict-hash)
            1975)
         (prin1-to-string
          (mapcar
           (lambda (word)
             (list
              word
              (adict-evaluate-word word)
              (adict-guess-word-language word)))
           '("HELLO"
             "zunächst"
             "bonjour"
             "además"
             "svenska"
             "pozdrav"
             "talán"
             "oricare"
             "obrigado"
             "aftenposten"
             "afskrækkende"
             "ἐγώγε"
             "καλημέρα"
             "भारत"
             "noreg"
             "felicitacions"
             "morgaŭ"
             "človek"
             "unclassified-token"))))"##,
        expect![[
            r#"OK (t t "((\"HELLO\" 1 \"en\") (\"zunächst\" 2 \"de\") (\"bonjour\" 3 \"fr\") (\"además\" 4 \"es\") (\"svenska\" 5 \"sv\") (\"pozdrav\" 6 \"sl\") (\"talán\" 7 \"hu\") (\"oricare\" 8 \"ro\") (\"obrigado\" 9 \"pt\") (\"aftenposten\" 10 \"nb\") (\"afskrækkende\" 11 \"da\") (\"ἐγώγε\" 12 \"grc\") (\"καλημέρα\" 13 \"el\") (\"भारत\" 14 \"hi\") (\"noreg\" 15 \"nn\") (\"felicitacions\" 16 \"ca\") (\"morgaŭ\" 17 \"eo\") (\"človek\" 18 \"sk\") (\"unclassified-token\" 0 nil))")"#
        ]],
    )
}

fn auto_dictionary_source_load_history_records_data_functions_aliases_and_provider()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dictionary_source_load_history_records_data_functions_aliases_and_provider",
        r##"(let* ((file
                 (locate-library
                  "auto-dictionary"))
                (history
                 (cdr
                  (assoc file load-history))))
         (seq-filter
          (lambda (event)
            (and
             (consp event)
             (or
              (memq
               (car event)
               '(provide defun defvar defface))
              (and
               (eq (car event) 'define-symbol-props)
               (memq
                (cadr event)
                '(adict-mode
                  switch-language-hook))))))
          history))"##,
        expect![
            "OK ((defface . adict-conditional-text-face) (defun . adict-guess-dictionary-name) (defun . adict--guess-dictionary-cons) (defun . adict--dictionary-alist-type) (defun . switch-language-hook) (defun . auto-dictionary-mode) (defun . adict-mode) (defun . adict-guess-dictionary) (defun . adict--cancel-timer) (defun . adict-valid-dictionary-p) (defun . adict-change-dictionary) (defun . adict-guess-dictionary-maybe) (defun . adict--next-guess-tick) (defun . adict-update-lighter) (defun . adict--shorten-dict) (defun . adict-foreach-word) (defun . adict-add-word) (defun . adict-evaluate-word) (defun . adict-evaluate-buffer) (defun . adict--evaluate-buffer-find-max-index) (defun . adict--evaluate-buffer-find-dictionary) (defun . adict--evaluate-buffer-find-lang) (defun . adict-conditional-insert) (defun . adict-conditional-insert-1) (defun . adict-conditional-modification) (defun . adict-conditional-update) (defun . adict-guess-word-language) (defun . adict-guess-buffer-language) (provide . auto-dictionary))"
        ],
    )
}

fn auto_dictionary_reload_preserves_customization_and_buffer_local_runtime_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_reload_preserves_customization_and_buffer_local_runtime_state",
        r##"(let ((source
                (locate-library
                 "auto-dictionary"))
               (adict-idle-time 17)
               (adict-change-threshold 0.5)
               (adict-dictionary-list
                '(("en" . "custom-en"))))
         (with-temp-buffer
           (setq-local adict-lighter " custom"
                       adict-last-check 42)
           (load source nil t t)
           (load source nil t t)
           (list
            adict-idle-time
            adict-change-threshold
            adict-dictionary-list
            adict-lighter
            adict-last-check
            (featurep 'auto-dictionary)
            (local-variable-p
             'adict-conditional-overlay-list))))"##,
        expect![[r#"OK (17 0.5 (("en" . "custom-en")) " custom" 42 t nil)"#]],
    )
}

fn auto_dictionary_generated_autoloads_register_commands_without_loading_source() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dictionary_generated_autoloads_register_commands_without_loading_source",
        r##"(let* ((file
                 (locate-library
                  "auto-dictionary-autoloads"))
                (history
                 (cdr
                  (assoc file load-history))))
         (list
          (featurep
           'auto-dictionary-autoloads)
          (featurep 'auto-dictionary)
          (seq-filter
           (lambda (event)
             (memq
              (car-safe event)
              '(defun provide)))
           history)
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (fboundp symbol)
              (autoloadp
               (symbol-function symbol))
              (commandp symbol)))
           '(auto-dictionary-mode
             adict-guess-dictionary
             adict-change-dictionary))))"##,
        expect![
            "OK (t nil ((defun . auto-dictionary-mode) (defun . adict-guess-dictionary) (defun . adict-change-dictionary) (provide . auto-dictionary-autoloads)) ((auto-dictionary-mode t t t) (adict-guess-dictionary t t t) (adict-change-dictionary t t t)))"
        ],
    )
}

pub(super) fn registry_auto_dictionary_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dictionary_descriptor_and_sources_pin_exact_melpa_payload(),
        auto_dictionary_feature_aliases_and_definition_origins_are_exact(),
        auto_dictionary_public_entry_points_have_exact_interactive_contracts(),
        auto_dictionary_options_language_order_and_deterministic_defaults_are_exact(),
        auto_dictionary_word_table_covers_each_language_and_unknown_bucket(),
        auto_dictionary_source_load_history_records_data_functions_aliases_and_provider(),
        auto_dictionary_reload_preserves_customization_and_buffer_local_runtime_state(),
    ]
}

pub(super) fn registry_auto_dictionary_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_dictionary_generated_autoloads_register_commands_without_loading_source()]
}
