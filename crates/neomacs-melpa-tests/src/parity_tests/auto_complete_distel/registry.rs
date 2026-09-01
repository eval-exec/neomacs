use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_distel_exact_descriptor_and_companion_dependency_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_exact_descriptor_and_companion_dependency_match",
        r##"(mapcar
          (lambda (package)
            (let ((descriptor
                   (cadr
                    (assq package
                          package-alist))))
              (list
               (package-desc-name descriptor)
               (package-version-join
                (package-desc-version descriptor))
               (package-desc-summary descriptor)
               (package-desc-reqs descriptor)
               (package-desc-extras descriptor))))
          '(auto-complete-distel
            distel-completion-lib
            auto-complete
            popup))"##,
        expect![[
            r#"OK ((auto-complete-distel "20180827.1344" "Erlang/distel completion backend for auto-complete-mode." ((auto-complete (1 4)) (distel-completion-lib (1 0 0))) ((:keywords "erlang" "distel" "auto-complete") (:revdesc . "acc4c0a55219") (:commit . "acc4c0a5521904203d797fe96b08e5fae4233c7e") (:url . "github.com/sebastiw/distel-completion"))) (distel-completion-lib "20180827.1344" "Completion library for Erlang/Distel." nil ((:keywords "erlang" "distel" "completion") (:revdesc . "acc4c0a55219") (:commit . "acc4c0a5521904203d797fe96b08e5fae4233c7e") (:url . "github.com/sebastiw/distel-completion"))) (auto-complete "20251231.1622" "Auto Completion for GNU Emacs." ((emacs (25 1)) (popup (0 5 8))) ((:maintainers ("Jen-Chieh Shen" . "jcs090218@gmail.com")) (:authors ("Tomohiro Matsuyama" . "m2ym.pub@gmail.com")) (:keywords "completion" "convenience") (:revdesc . "07f9915e0834") (:commit . "07f9915e08342410b933145d7934998709753a29") (:url . "https://github.com/auto-complete/auto-complete"))) (popup "20251231.1622" "Visual Popup User Interface." ((emacs (24 3))) ((:maintainers ("Jen-Chieh" . "jcs090218@gmail.com")) (:authors ("Tomohiro Matsuyama" . "m2ym.pub@gmail.com")) (:keywords "lisp") (:revdesc . "45a0b759076c") (:commit . "45a0b759076ce4139aba36dde0a2904136282e73") (:url . "https://github.com/auto-complete/popup-el"))))"#
        ]],
    )
}

fn auto_complete_distel_exact_installed_payload_bytes_match_melpa_archives() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_exact_installed_payload_bytes_match_melpa_archives",
        r##"(mapcar
          (lambda (fixture)
            (let* ((package
                    (car fixture))
                   (descriptor
                    (cadr
                     (assq package
                           package-alist)))
                   (directory
                    (package-desc-dir descriptor)))
              (cons
               package
               (mapcar
                (lambda (name)
                  (let ((file
                         (expand-file-name
                          name directory)))
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
                (cdr fixture)))))
          '((auto-complete-distel
             "auto-complete-distel-pkg.el"
             "auto-complete-distel.el")
            (distel-completion-lib
             "distel-completion-lib-pkg.el"
             "distel-completion-lib.el")))"##,
        expect![[
            r#"OK ((auto-complete-distel ("auto-complete-distel-pkg.el" 415 "19f9edabfe643d9245e7a14f35714ed501dcced987a9211fdbf4f2e58bf80696") ("auto-complete-distel.el" 1584 "c6d0e71b57662604e963cdaca992f2ed5851679d7310e5ff314f8c83c3089068")) (distel-completion-lib ("distel-completion-lib-pkg.el" 328 "86d936b2c0bb5b20563c9ac62a2d22cbbed6bc2df81c4cbc3a2c9e1f424f87c3") ("distel-completion-lib.el" 9247 "89a2e03906cdcae7f04ec032039d8e9cb1d91b4f964ec94dc406873f250d80e7")))"#
        ]],
    )
}

fn auto_complete_distel_complete_target_symbol_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_complete_target_symbol_surface_matches",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (when
                 (and
                  (string-prefix-p
                   "auto-complete-distel"
                   (symbol-name symbol))
                  (equal
                   (file-name-nondirectory
                    (or
                     (symbol-file symbol)
                     ""))
                   "auto-complete-distel.el"))
               (push
                (list
                 symbol
                 (fboundp symbol)
                 (boundp symbol)
                 (and
                  (commandp symbol)
                  t)
                 (and
                  (local-variable-if-set-p symbol)
                  t)
                 (file-name-nondirectory
                  (or
                   (symbol-file symbol)
                   "")))
                symbols))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name
               (car left))
              (symbol-name
               (car right))))))"##,
        expect![[
            r#"OK ((auto-complete-distel nil t nil nil "auto-complete-distel.el") (auto-complete-distel-get-start t nil nil nil "auto-complete-distel.el"))"#
        ]],
    )
}

fn auto_complete_distel_callable_and_variable_contracts_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_callable_and_variable_contracts_match",
        r##"(list
          (list
           'auto-complete-distel-get-start
           (help-function-arglist
            'auto-complete-distel-get-start
            t)
           (commandp
            'auto-complete-distel-get-start)
           (interactive-form
            'auto-complete-distel-get-start)
           (documentation
            'auto-complete-distel-get-start)
           (file-name-nondirectory
            (symbol-file
             'auto-complete-distel-get-start
             'defun)))
          (list
           'auto-complete-distel
           auto-complete-distel
           (documentation-property
            'auto-complete-distel
            'variable-documentation)
           (local-variable-if-set-p
            'auto-complete-distel)
           (file-name-nondirectory
            (symbol-file
             'auto-complete-distel
             'defvar))))"##,
        expect![[
            r#"OK ((auto-complete-distel-get-start nil nil nil "Find a valid start of a completion word." "auto-complete-distel.el") (auto-complete-distel ((prefix . auto-complete-distel-get-start) (candidates distel-completion-complete ac-prefix (current-buffer)) (document . distel-completion-get-doc-string) (requires . 0) (symbol . "m")) "All it takes to start a auto-complete backend." nil "auto-complete-distel.el"))"#
        ]],
    )
}

fn auto_complete_distel_source_entries_are_exact_and_resolve_to_callable_behavior()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_source_entries_are_exact_and_resolve_to_callable_behavior",
        r##"(let ((prefix
                                (cdr
                                 (assq 'prefix
                                       auto-complete-distel)))
                               (candidates
                                (cdr
                                 (assq 'candidates
                                       auto-complete-distel)))
                               (document
                                (cdr
                                 (assq 'document
                                       auto-complete-distel))))
          (list
           auto-complete-distel
           prefix
           candidates
           document
           (functionp prefix)
           (functionp document)
           (cdr
            (assq 'requires
                  auto-complete-distel))
           (cdr
            (assq 'symbol
                  auto-complete-distel))))"##,
        expect![[
            r#"OK (((prefix . auto-complete-distel-get-start) (candidates . #1=(distel-completion-complete ac-prefix (current-buffer))) (document . distel-completion-get-doc-string) (requires . 0) (symbol . "m")) auto-complete-distel-get-start #1# distel-completion-get-doc-string t t 0 "m")"#
        ]],
    )
}

fn auto_complete_distel_load_history_records_the_complete_target_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_load_history_records_the_complete_target_contract",
        r##"(let* ((file
                                 (locate-library
                                  "auto-complete-distel"))
                                (history
                                 (cdr
                                  (assoc file
                                         load-history))))
          (list
           (file-name-nondirectory file)
           (seq-filter
            (lambda (event)
              (and
               (consp event)
               (memq
                (car event)
                '(require defun provide))))
            history)
           (featurep
            'auto-complete-distel)
           (featurep
            'distel-completion-lib)
           (featurep 'distel)))"##,
        expect![[
            r#"OK ("auto-complete-distel.el" ((require . auto-complete) (require . distel-completion-lib) (defun . auto-complete-distel-get-start) (provide . auto-complete-distel)) t t t)"#
        ]],
    )
}

fn auto_complete_distel_companion_library_surface_and_defaults_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_companion_library_surface_and_defaults_match",
        r##"(let (functions)
          (mapatoms
           (lambda (symbol)
             (when
                 (equal
                  (file-name-nondirectory
                   (or
                    (symbol-file symbol 'defun)
                    ""))
                  "distel-completion-lib.el")
               (push
                (list
                 symbol
                 (help-function-arglist
                  symbol t)
                 (and
                  (commandp symbol)
                  t))
                functions))))
          (list
           (sort
            functions
            (lambda (left right)
              (string<
               (symbol-name
                (car left))
               (symbol-name
                (car right)))))
           (list
            distel-completion-get-doc-from-internet
            (get
             'distel-completion-get-doc-from-internet
             'custom-type)
            (documentation-property
             'distel-completion-get-doc-from-internet
             'variable-documentation))
           (list
            distel-completion-valid-syntax
            (get
             'distel-completion-valid-syntax
             'custom-type)
            (documentation-property
             'distel-completion-valid-syntax
             'variable-documentation))
           (list
            distel-completion-try-erl-args-cache
            distel-completion-try-erl-desc-cache
            distel-completion-try-erl-complete-cache)))"##,
        expect![[
            r#"OK (((&distel-completion-receive-args nil nil) (&distel-completion-receive-completions nil nil) (&distel-completion-receive-describe (args) nil) (distel-completion-args (mod fun) nil) (distel-completion-complete (search-string buf) nil) (distel-completion-complete-function (module function) nil) (distel-completion-complete-module (module) nil) (distel-completion-describe (mod fun args) nil) (distel-completion-get-dabbrevs (args &optional times limit) nil) (distel-completion-get-doc-buffer (args) nil) (distel-completion-get-doc-string (args) t) (distel-completion-get-docs-from-internet-p (mod fun) nil) (distel-completion-get-functions (args) nil) (distel-completion-get-metadoc (mod fun) nil) (distel-completion-grab-word nil t) (distel-completion-html-to-string (string) nil) (distel-completion-is-comment-or-cite-p (&optional poin) nil) (distel-completion-local-docs (mod fun) nil)) (t nil "Try to find the documentation from erlang.org") ("a-zA-Z:_-" nil "Which syntax to skip backwards to find start of word.") (nil "" nil))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_distel_generated_autoload_contains_only_feature_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_generated_autoload_contains_only_feature_contract",
        r##"(let* ((file
                                 (locate-library
                                  "auto-complete-distel-autoloads"))
                                (history
                                 (cdr
                                  (assoc file
                                         load-history))))
          (list
           (featurep
            'auto-complete-distel-autoloads)
           (featurep
            'auto-complete-distel)
           (boundp
            'auto-complete-distel)
           (fboundp
            'auto-complete-distel-get-start)
           (seq-filter
            (lambda (event)
              (memq
               (car-safe event)
               '(defun defvar provide)))
            history)))"##,
        expect!["OK (t nil nil nil ((provide . auto-complete-distel-autoloads)))"],
    )
}

pub(super) fn registry_auto_complete_distel_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_distel_exact_descriptor_and_companion_dependency_match(),
        auto_complete_distel_exact_installed_payload_bytes_match_melpa_archives(),
        auto_complete_distel_complete_target_symbol_surface_matches(),
        auto_complete_distel_callable_and_variable_contracts_match(),
        auto_complete_distel_source_entries_are_exact_and_resolve_to_callable_behavior(),
        auto_complete_distel_load_history_records_the_complete_target_contract(),
        auto_complete_distel_companion_library_surface_and_defaults_match(),
    ]
}

pub(super) fn registry_auto_complete_distel_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_distel_generated_autoload_contains_only_feature_contract()]
}
