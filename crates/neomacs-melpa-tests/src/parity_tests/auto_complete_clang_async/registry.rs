use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_async_exact_descriptor_provenance_and_manual_dependencies_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_exact_descriptor_provenance_and_manual_dependencies_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-complete-clang-async
                                   package-alist)))
                                (auto-complete-descriptor
                                 (cadr
                                  (assq
                                   'auto-complete
                                   package-alist)))
                                (popup-descriptor
                                 (cadr
                                  (assq
                                   'popup
                                   package-alist))))
                           (list
                            (package-desc-name descriptor)
                            (package-version-join
                             (package-desc-version descriptor))
                            (package-desc-summary descriptor)
                            (package-desc-reqs descriptor)
                            (package-desc-extras descriptor)
                            (list
                             (package-desc-name
                              auto-complete-descriptor)
                             (package-version-join
                              (package-desc-version
                               auto-complete-descriptor)))
                            (list
                             (package-desc-name
                              popup-descriptor)
                             (package-version-join
                              (package-desc-version
                               popup-descriptor)))
                            (featurep
                             'auto-complete-clang-async)
                            (featurep 'auto-complete)
                            (featurep 'popup)
                            (featurep 'flymake)))"##,
        expect![[
            r#"OK (auto-complete-clang-async "20130526.1527" "Auto Completion source for clang for GNU Emacs." nil ((:maintainers ("Brian Jiang" . "brianjcj@gmail.com") ("Taylan Ulrich Bayirli/Kammer" . "taylanbayirli@gmail.com")) (:authors ("Brian Jiang" . "brianjcj@gmail.com") ("Taylan Ulrich Bayirli/Kammer" . "taylanbayirli@gmail.com")) (:keywords "completion" "convenience") (:revdesc . "a5114e347779") (:commit . "a5114e3477793ccb9420acc5cd6a1cb26be65964") (:url . "https://github.com/Golevka/emacs-clang-complete-async")) (auto-complete "20251231.1622") (popup "20251231.1622") t t t t)"#
        ]],
    )
}

fn auto_complete_clang_async_installed_payload_inventory_and_exact_hashes_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_async_installed_payload_inventory_and_exact_hashes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-complete-clang-async
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor))
                                (files
                                 (sort
                                  (directory-files
                                   directory
                                   nil
                                   "auto-complete-clang-async.*\\.el\\'")
                                  #'string<)))
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
                            files))"##,
        expect![[
            r#"OK (("auto-complete-clang-async-autoloads.el" 768 "f23a6df2e610d16d68111ed730ddf9bc4b5d4bb41a19a1cc33ea39add52ccc7e") ("auto-complete-clang-async-pkg.el" 616 "ab3ee5480362c49a04243c1fa7d6f2c5dc47c6716e1d798c9cbad7420e4bfb25") ("auto-complete-clang-async.el" 23579 "5e9c97c000fc805f11aeaa519b247687b113fde7b777e46eca3a473afb9b5ace"))"#
        ]],
    )
}

fn auto_complete_clang_async_complete_prefixed_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_complete_prefixed_symbol_inventory_matches",
        r##"(let (symbols)
                           (mapatoms
                            (lambda (symbol)
                              (let ((name
                                     (symbol-name symbol)))
                                (when
                                    (and
                                     (or
                                      (string-prefix-p
                                       "ac-clang-"
                                       name)
                                      (string-prefix-p
                                       "ac-source-clang"
                                       name))
                                     (not
                                      (string-prefix-p
                                       "acclang-test-"
                                       name)))
                                  (push
                                   (list
                                    symbol
                                    (fboundp symbol)
                                    (boundp symbol)
                                    (and
                                     (commandp symbol)
                                     t)
                                    (and
                                     (facep symbol)
                                     t)
                                    (local-variable-if-set-p
                                     symbol)
                                    (file-name-nondirectory
                                     (or
                                      (symbol-file symbol)
                                      "")))
                                   symbols)))))
                           (sort
                            symbols
                            (lambda (left right)
                              (string<
                               (symbol-name
                                (car left))
                               (symbol-name
                                (car right))))))"##,
        expect![[
            r#"OK ((ac-clang-action t nil t nil nil "auto-complete-clang-async.el") (ac-clang-append-process-output-to-process-buffer t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-async-autocomplete-autotrigger t nil t nil nil "auto-complete-clang-async.el") (ac-clang-async-do-autocompletion-automatically nil t nil nil nil "auto-complete-clang-async.el") (ac-clang-async-preemptive t nil t nil nil "auto-complete-clang-async.el") (ac-clang-build-complete-args t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-call-process t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-candidate t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-candidate-face nil nil nil t nil "auto-complete-clang-async.el") (ac-clang-cflags nil t nil nil t "auto-complete-clang-async.el") (ac-clang-clean-document t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-complete-executable nil t nil nil nil "auto-complete-clang-async.el") (ac-clang-completion-pattern nil t nil nil nil "auto-complete-clang-async.el") (ac-clang-completion-process nil t nil nil t "auto-complete-clang-async.el") (ac-clang-create-position-string t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-current-candidate nil t nil nil t "auto-complete-clang-async.el") (ac-clang-document t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-error-buffer-name nil t nil nil nil "auto-complete-clang-async.el") (ac-clang-filter-output t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-flymake-process-filter t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-flymake-process-sentinel t nil t nil nil "auto-complete-clang-async.el") (ac-clang-handle-error t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-help nil nil nil nil nil "") (ac-clang-in-string/comment t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-lang-option t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-lang-option-function nil t nil nil nil "auto-complete-clang-async.el") (ac-clang-launch-completion-process t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-launch-completion-process-with-file t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-parse-completion-results t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-parse-output t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-prefix t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-prefix-header nil t nil nil t "auto-complete-clang-async.el") (ac-clang-reparse-buffer t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-same-count-in-string t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-saved-prefix nil t nil nil nil "auto-complete-clang-async.el") (ac-clang-selection-face nil nil nil t nil "auto-complete-clang-async.el") (ac-clang-send-cmdline-args t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-send-completion-request t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-send-reparse-request t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-send-shutdown-command t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-send-source-code t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-send-syntaxcheck-request t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-set-cflags t nil t nil nil "auto-complete-clang-async.el") (ac-clang-set-cflags-from-shell-command t nil t nil nil "auto-complete-clang-async.el") (ac-clang-set-prefix-header t nil t nil nil "auto-complete-clang-async.el") (ac-clang-shutdown-process t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-split-args t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-status nil t nil nil t "auto-complete-clang-async.el") (ac-clang-syntax-check t nil t nil nil "auto-complete-clang-async.el") (ac-clang-template-action t nil t nil nil "auto-complete-clang-async.el") (ac-clang-template-candidate t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-template-candidates nil t nil nil nil "auto-complete-clang-async.el") (ac-clang-template-prefix t nil nil nil nil "auto-complete-clang-async.el") (ac-clang-template-start-point nil t nil nil nil "auto-complete-clang-async.el") (ac-clang-update-cmdlineargs t nil t nil nil "auto-complete-clang-async.el") (ac-source-clang-async nil t nil nil nil "") (ac-source-clang-template nil t nil nil nil ""))"#
        ]],
    )
}

fn auto_complete_clang_async_every_callable_arglist_interactivity_documentation_and_origin_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_every_callable_arglist_interactivity_documentation_and_origin_match",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (help-function-arglist
                               symbol
                               t)
                              (and
                               (interactive-form symbol)
                               t)
                              (and
                               (commandp symbol)
                               t)
                              (documentation symbol t)
                              (file-name-nondirectory
                               (or
                                (symbol-file
                                 symbol
                                 'defun)
                                ""))))
                           '(ac-clang-set-cflags
                             ac-clang-set-cflags-from-shell-command
                             ac-clang-set-prefix-header
                             ac-clang-parse-output
                             ac-clang-handle-error
                             ac-clang-call-process
                             ac-clang-create-position-string
                             ac-clang-lang-option
                             ac-clang-build-complete-args
                             ac-clang-clean-document
                             ac-clang-document
                             ac-clang-in-string/comment
                             ac-clang-action
                             ac-clang-prefix
                             ac-clang-same-count-in-string
                             ac-clang-split-args
                             ac-clang-template-candidate
                             ac-clang-template-action
                             ac-clang-template-prefix
                             ac-clang-send-source-code
                             ac-clang-send-reparse-request
                             ac-clang-send-completion-request
                             ac-clang-send-syntaxcheck-request
                             ac-clang-send-cmdline-args
                             ac-clang-update-cmdlineargs
                             ac-clang-send-shutdown-command
                             ac-clang-append-process-output-to-process-buffer
                             ac-clang-parse-completion-results
                             ac-clang-filter-output
                             ac-clang-candidate
                             ac-clang-flymake-process-sentinel
                             ac-clang-flymake-process-filter
                             ac-clang-syntax-check
                             ac-clang-shutdown-process
                             ac-clang-reparse-buffer
                             ac-clang-async-autocomplete-autotrigger
                             ac-clang-async-preemptive
                             ac-clang-launch-completion-process
                             ac-clang-launch-completion-process-with-file))"##,
        expect![[
            r#"OK ((ac-clang-set-cflags nil t t "Set `ac-clang-cflags' interactively." "auto-complete-clang-async.el") (ac-clang-set-cflags-from-shell-command nil t t "Set `ac-clang-cflags' to a shell command's output.\n\nset new cflags for ac-clang from shell command output" "auto-complete-clang-async.el") (ac-clang-set-prefix-header (prefix-header) t t "Set `ac-clang-prefix-header' interactively." "auto-complete-clang-async.el") (ac-clang-parse-output (prefix) nil nil nil "auto-complete-clang-async.el") (ac-clang-handle-error (res args) nil nil nil "auto-complete-clang-async.el") (ac-clang-call-process (prefix &rest args) nil nil nil "auto-complete-clang-async.el") (ac-clang-create-position-string (pos) nil nil nil "auto-complete-clang-async.el") (ac-clang-lang-option nil nil nil nil "auto-complete-clang-async.el") (ac-clang-build-complete-args nil nil nil nil "auto-complete-clang-async.el") (ac-clang-clean-document (s) nil nil nil "auto-complete-clang-async.el") (ac-clang-document (item) nil nil nil "auto-complete-clang-async.el") (ac-clang-in-string/comment nil nil nil "Return non-nil if point is in a literal (a comment or string)." "auto-complete-clang-async.el") (ac-clang-action nil t t nil "auto-complete-clang-async.el") (ac-clang-prefix nil nil nil nil "auto-complete-clang-async.el") (ac-clang-same-count-in-string (c1 c2 s) nil nil nil "auto-complete-clang-async.el") (ac-clang-split-args (s) nil nil nil "auto-complete-clang-async.el") (ac-clang-template-candidate nil nil nil nil "auto-complete-clang-async.el") (ac-clang-template-action nil t t nil "auto-complete-clang-async.el") (ac-clang-template-prefix nil nil nil nil "auto-complete-clang-async.el") (ac-clang-send-source-code (proc) nil nil nil "auto-complete-clang-async.el") (ac-clang-send-reparse-request (proc) nil nil nil "auto-complete-clang-async.el") (ac-clang-send-completion-request (proc) nil nil nil "auto-complete-clang-async.el") (ac-clang-send-syntaxcheck-request (proc) nil nil nil "auto-complete-clang-async.el") (ac-clang-send-cmdline-args (proc) nil nil nil "auto-complete-clang-async.el") (ac-clang-update-cmdlineargs nil t t nil "auto-complete-clang-async.el") (ac-clang-send-shutdown-command (proc) nil nil nil "auto-complete-clang-async.el") (ac-clang-append-process-output-to-process-buffer (process output) nil nil "Append process output to the process buffer." "auto-complete-clang-async.el") (ac-clang-parse-completion-results (proc) nil nil nil "auto-complete-clang-async.el") (ac-clang-filter-output (proc string) nil nil nil "auto-complete-clang-async.el") (ac-clang-candidate nil nil nil nil "auto-complete-clang-async.el") (ac-clang-flymake-process-sentinel nil t t nil "auto-complete-clang-async.el") (ac-clang-flymake-process-filter (process output) nil nil nil "auto-complete-clang-async.el") (ac-clang-syntax-check nil t t nil "auto-complete-clang-async.el") (ac-clang-shutdown-process nil nil nil nil "auto-complete-clang-async.el") (ac-clang-reparse-buffer nil nil nil nil "auto-complete-clang-async.el") (ac-clang-async-autocomplete-autotrigger nil t t nil "auto-complete-clang-async.el") (ac-clang-async-preemptive nil t t nil "auto-complete-clang-async.el") (ac-clang-launch-completion-process nil nil nil nil "auto-complete-clang-async.el") (ac-clang-launch-completion-process-with-file (filename) nil nil nil "auto-complete-clang-async.el"))"#
        ]],
    )
}

fn auto_complete_clang_async_custom_and_internal_defaults_types_locality_and_sources_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_custom_and_internal_defaults_types_locality_and_sources_match",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (and
                               (boundp symbol)
                               (symbol-value symbol))
                              (and
                               (boundp symbol)
                               (default-value symbol))
                              (get symbol 'standard-value)
                              (get symbol 'custom-type)
                              (get symbol 'custom-group)
                              (local-variable-if-set-p
                               symbol)
                              (documentation-property
                               symbol
                               'variable-documentation
                               t)
                              (file-name-nondirectory
                               (or
                                (symbol-file
                                 symbol
                                 'defvar)
                                ""))))
                           '(ac-clang-complete-executable
                             ac-clang-lang-option-function
                             ac-clang-cflags
                             ac-clang-prefix-header
                             ac-clang-async-do-autocompletion-automatically
                             ac-clang-completion-pattern
                             ac-clang-error-buffer-name
                             ac-clang-template-start-point
                             ac-clang-template-candidates
                             ac-clang-status
                             ac-clang-current-candidate
                             ac-clang-completion-process
                             ac-clang-saved-prefix
                             ac-source-clang-template
                             ac-source-clang-async))"##,
        expect![[
            r#"OK ((ac-clang-complete-executable nil nil ((executable-find "clang-complete")) file nil nil "Location of clang-complete executable." "auto-complete-clang-async.el") (ac-clang-lang-option-function nil nil (nil) function nil nil "Function to return the lang type for option -x." "auto-complete-clang-async.el") (ac-clang-cflags nil nil (nil) (repeat (string :tag "Argument" "")) nil t "Extra flags to pass to the Clang executable.\nThis variable will typically contain include paths, e.g., (\"-I~/MyProject\" \"-I.\")." "auto-complete-clang-async.el") (ac-clang-prefix-header nil nil nil nil nil t "The prefix header to pass to the Clang executable." "auto-complete-clang-async.el") (ac-clang-async-do-autocompletion-automatically t t nil nil nil nil "If autocompletion is automatically triggered when you type ., -> or ::" "auto-complete-clang-async.el") (ac-clang-completion-pattern "^COMPLETION: \\(%s[^ \n:]*\\)\\(?: : \\)*\\(.*$\\)" "^COMPLETION: \\(%s[^ \n:]*\\)\\(?: : \\)*\\(.*$\\)" nil nil nil nil nil "auto-complete-clang-async.el") (ac-clang-error-buffer-name "*clang error*" "*clang error*" nil nil nil nil nil "auto-complete-clang-async.el") (ac-clang-template-start-point nil nil nil nil nil nil nil "auto-complete-clang-async.el") (ac-clang-template-candidates #1=("ok" "no" "yes:)") #1# nil nil nil nil nil "auto-complete-clang-async.el") (ac-clang-status idle idle nil nil nil t nil "auto-complete-clang-async.el") (ac-clang-current-candidate nil nil nil nil nil t nil "auto-complete-clang-async.el") (ac-clang-completion-process nil nil nil nil nil t nil "auto-complete-clang-async.el") (ac-clang-saved-prefix "" "" nil nil nil nil nil "auto-complete-clang-async.el") (ac-source-clang-template #2=((candidates . ac-clang-template-candidate) (prefix . ac-clang-template-prefix) (requires . 0) (action . ac-clang-template-action) (document . ac-clang-document) (cache) (symbol . "t")) #2# nil nil nil nil nil "") (ac-source-clang-async #3=((candidates . ac-clang-candidate) (candidate-face . ac-clang-candidate-face) (selection-face . ac-clang-selection-face) (prefix . ac-clang-prefix) (requires . 0) (document . ac-clang-document) (action . ac-clang-action) (cache) (symbol . "c")) #3# nil nil nil nil nil ""))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_clang_async_faces_and_completion_source_contracts_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_faces_and_completion_source_contracts_match",
        r##"(list
                           (mapcar
                            (lambda (face)
                              (list
                               face
                               (get face
                                    'face-defface-spec)
                               (face-documentation face)
                               (get face 'custom-group)
                               (file-name-nondirectory
                                (or
                                 (symbol-file face
                                              'defface)
                                 ""))))
                            '(ac-clang-candidate-face
                              ac-clang-selection-face))
                           ac-source-clang-template
                           ac-source-clang-async
                           (mapcar
                            (lambda (source)
                              (mapcar
                               (lambda (property)
                                 (let ((value
                                        (cdr
                                         (assq
                                          property
                                          (symbol-value
                                           source)))))
                                   (list
                                    property
                                    value
                                    (and
                                     (symbolp value)
                                     (fboundp value)))))
                               '(candidates
                                 prefix
                                 action
                                 document)))
                            '(ac-source-clang-template
                              ac-source-clang-async)))"##,
        expect![[
            r#"OK (((ac-clang-candidate-face ((t (:background "lightgray" :foreground "navy"))) "Face for clang candidate" nil "auto-complete-clang-async.el") (ac-clang-selection-face ((t (:background "navy" :foreground "white"))) "Face for the clang selected candidate." nil "auto-complete-clang-async.el")) ((candidates . ac-clang-template-candidate) (prefix . ac-clang-template-prefix) (requires . 0) (action . ac-clang-template-action) (document . ac-clang-document) (cache) (symbol . "t")) ((candidates . ac-clang-candidate) (candidate-face . ac-clang-candidate-face) (selection-face . ac-clang-selection-face) (prefix . ac-clang-prefix) (requires . 0) (document . ac-clang-document) (action . ac-clang-action) (cache) (symbol . "c")) (((candidates ac-clang-template-candidate t) (prefix ac-clang-template-prefix t) (action ac-clang-template-action t) (document ac-clang-document t)) ((candidates ac-clang-candidate t) (prefix ac-clang-prefix t) (action ac-clang-action t) (document ac-clang-document t))))"#
        ]],
    )
}

fn auto_complete_clang_async_source_load_history_records_complete_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_source_load_history_records_complete_contract",
        r##"(let* ((file
                                 (locate-library
                                  "auto-complete-clang-async"))
                                (history
                                 (cdr
                                  (assoc file
                                         load-history))))
                           (seq-filter
                            (lambda (event)
                              (and
                               (consp event)
                               (memq
                                (car event)
                                '(require
                                  provide
                                  defun
                                  defvar
                                  defface))))
                            history))"##,
        expect![
            "OK ((provide . auto-complete-clang-async) (require . cl) (require . auto-complete) (require . flymake) (defun . ac-clang-set-cflags) (defun . ac-clang-set-cflags-from-shell-command) (defun . ac-clang-set-prefix-header) (defun . ac-clang-parse-output) (defun . ac-clang-handle-error) (defun . ac-clang-call-process) (defun . ac-clang-create-position-string) (defun . ac-clang-lang-option) (defun . ac-clang-build-complete-args) (defun . ac-clang-clean-document) (defun . ac-clang-document) (defface . ac-clang-candidate-face) (defface . ac-clang-selection-face) (defun . ac-clang-in-string/comment) (defun . ac-clang-action) (defun . ac-clang-prefix) (defun . ac-clang-same-count-in-string) (defun . ac-clang-split-args) (defun . ac-clang-template-candidate) (defun . ac-clang-template-action) (defun . ac-clang-template-prefix) (defun . ac-complete-clang-template) (defun . ac-clang-send-source-code) (defun . ac-clang-send-reparse-request) (defun . ac-clang-send-completion-request) (defun . ac-clang-send-syntaxcheck-request) (defun . ac-clang-send-cmdline-args) (defun . ac-clang-update-cmdlineargs) (defun . ac-clang-send-shutdown-command) (defun . ac-clang-append-process-output-to-process-buffer) (defun . ac-clang-parse-completion-results) (defun . ac-clang-filter-output) (defun . ac-clang-candidate) (defun . ac-clang-flymake-process-sentinel) (defun . ac-clang-flymake-process-filter) (defun . ac-clang-syntax-check) (defun . ac-clang-shutdown-process) (defun . ac-clang-reparse-buffer) (defun . ac-clang-async-autocomplete-autotrigger) (defun . ac-clang-async-preemptive) (defun . ac-clang-launch-completion-process) (defun . ac-clang-launch-completion-process-with-file) (defun . ac-complete-clang-async))"
        ],
    )
}

fn auto_complete_clang_async_source_reload_preserves_user_state_but_redefines_functions_and_sources()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_source_reload_preserves_user_state_but_redefines_functions_and_sources",
        r##"(let ((source
                                (locate-library
                                 "auto-complete-clang-async"))
                               (ac-clang-complete-executable
                                "./custom-clang-complete")
                               (ac-clang-cflags
                                '("-Icustom"
                                  "-DVALUE=7"))
                               (ac-clang-prefix-header
                                "./prefix.pch")
                               (before
                                (symbol-function
                                 'ac-clang-prefix)))
                           (setq
                            ac-source-clang-async
                            '((sentinel . custom)))
                           (load source nil t t)
                           (list
                            ac-clang-complete-executable
                            ac-clang-cflags
                            ac-clang-prefix-header
                            (eq
                             before
                             (symbol-function
                              'ac-clang-prefix))
                            ac-source-clang-async))"##,
        expect![[
            r#"OK ("./custom-clang-complete" ("-Icustom" "-DVALUE=7") "./prefix.pch" nil ((candidates . ac-clang-candidate) (candidate-face . ac-clang-candidate-face) (selection-face . ac-clang-selection-face) (prefix . ac-clang-prefix) (requires . 0) (document . ac-clang-document) (action . ac-clang-action) (cache) (symbol . "c")))"#
        ]],
    )
}

fn auto_complete_clang_async_generated_autoload_contains_only_package_metadata_and_provide()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_generated_autoload_contains_only_package_metadata_and_provide",
        r##"(let* ((file
                                 (locate-library
                                  "auto-complete-clang-async-autoloads"))
                                (history
                                 (cdr
                                  (assoc file
                                         load-history))))
                           (list
                            (featurep
                             'auto-complete-clang-async-autoloads)
                            (featurep
                             'auto-complete-clang-async)
                            (seq-filter
                             (lambda (event)
                               (memq
                                (car-safe event)
                                '(defun
                                  defvar
                                  provide)))
                             history)
                            (fboundp
                             'ac-clang-candidate)
                            (boundp
                             'ac-source-clang-async)))"##,
        expect!["OK (t nil ((provide . auto-complete-clang-async-autoloads)) nil nil)"],
    )
}

pub(super) fn registry_auto_complete_clang_async_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_async_exact_descriptor_provenance_and_manual_dependencies_match(),
        auto_complete_clang_async_installed_payload_inventory_and_exact_hashes_match(),
        auto_complete_clang_async_complete_prefixed_symbol_inventory_matches(),
        auto_complete_clang_async_every_callable_arglist_interactivity_documentation_and_origin_match(),
        auto_complete_clang_async_custom_and_internal_defaults_types_locality_and_sources_match(),
        auto_complete_clang_async_faces_and_completion_source_contracts_match(),
        auto_complete_clang_async_source_load_history_records_complete_contract(),
        auto_complete_clang_async_source_reload_preserves_user_state_but_redefines_functions_and_sources(),
    ]
}

pub(super) fn registry_auto_complete_clang_async_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_clang_async_generated_autoload_contains_only_package_metadata_and_provide()]
}
