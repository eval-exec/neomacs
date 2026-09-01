use expect_test::expect;

use super::ParityBatchCase;

fn atomic_chrome_descriptor_and_archive_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_descriptor_and_archive_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'atomic-chrome
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name
                    name
                    directory))
                 '("atomic-chrome-pkg.el"
                   "atomic-chrome.el"))))
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
            r#"OK ((atomic-chrome "20230304.112" "Edit Chrome text area with Emacs using Atomic Chrome." ((emacs (24 4)) (let-alist (1 0 4)) (websocket (1 4))) ((:maintainers ("alpha22jp" . "alpha22jp@gmail.com")) (:authors ("alpha22jp" . "alpha22jp@gmail.com")) (:keywords "chrome" "edit" "textarea") (:revdesc . "f1b077be7e41") (:commit . "f1b077be7e414f457191d72dcf5eedb4371f9309") (:url . "https://github.com/alpha22jp/atomic-chrome"))) (("atomic-chrome-pkg.el" 509 "6b870dd9c4c31da000919a28ae19ac58a251fe71fc7fbf941bb2b0d299155494") ("atomic-chrome.el" 15626 "19a1b504e4c1a15bad0dbf27b6afb188c707042361008fa563b2d0c685273e3d")))"#
        ]],
    )
}

fn atomic_chrome_complete_prefixed_symbol_inventory_records_every_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_complete_prefixed_symbol_inventory_records_every_surface",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (string-prefix-p
                     "atomic-chrome"
                     name)
                    (not
                     (string-prefix-p
                      "atomic-chrome-test-"
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
            "OK ((atomic-chrome nil nil nil nil nil) (atomic-chrome-autoloads nil nil nil nil nil) (atomic-chrome-buffer-frame-height nil t t nil nil) (atomic-chrome-buffer-frame-width nil t t nil nil) (atomic-chrome-buffer-open-style nil t t nil nil) (atomic-chrome-buffer-table nil t nil nil nil) (atomic-chrome-close-connection t nil nil nil nil) (atomic-chrome-close-current-buffer t nil nil nil nil) (atomic-chrome-close-edit-buffer t nil nil nil (buffer)) (atomic-chrome-create-buffer t nil nil nil (socket url title text)) (atomic-chrome-default-major-mode nil t t nil nil) (atomic-chrome-edit-done-hook nil t t nil nil) (atomic-chrome-edit-mode t t nil nil (&optional arg)) (atomic-chrome-edit-mode--set-explicitly t t nil nil nil) (atomic-chrome-edit-mode--suppress-set-explicitly nil t nil nil nil) (atomic-chrome-edit-mode-hook nil t t nil nil) (atomic-chrome-edit-mode-map nil t nil nil nil) (atomic-chrome-edit-mode-off-hook nil nil nil nil nil) (atomic-chrome-edit-mode-on-hook nil nil nil nil nil) (atomic-chrome-enable-auto-update nil t t nil nil) (atomic-chrome-enable-bidirectional-edit nil t t nil nil) (atomic-chrome-extension-type-list nil t t nil nil) (atomic-chrome-get-buffer-by-socket t nil nil nil (socket)) (atomic-chrome-get-frame t nil nil nil (buffer)) (atomic-chrome-get-websocket t nil nil nil (buffer)) (atomic-chrome-httpd-parse-string t nil nil nil (string)) (atomic-chrome-httpd-process-filter t nil nil nil (proc string)) (atomic-chrome-httpd-send-response t nil nil nil (proc)) (atomic-chrome-normalize-header t nil nil nil (header)) (atomic-chrome-on-close t nil nil nil (socket)) (atomic-chrome-on-message t nil nil nil (socket frame)) (atomic-chrome-send-buffer-text t nil nil nil nil) (atomic-chrome-server-atomic-chrome nil t nil nil nil) (atomic-chrome-server-ghost-text nil t nil nil nil) (atomic-chrome-server-ghost-text-port nil t t nil nil) (atomic-chrome-set-major-mode t nil nil nil (url)) (atomic-chrome-show-edit-buffer t nil nil nil (buffer title)) (atomic-chrome-start-httpd t nil nil nil nil) (atomic-chrome-start-server t nil nil nil nil) (atomic-chrome-start-websocket-server t nil nil nil (port)) (atomic-chrome-stop-server t nil nil nil nil) (atomic-chrome-turn-on-edit-mode t nil nil nil nil) (atomic-chrome-update-buffer t nil nil nil (socket text)) (atomic-chrome-url-major-mode-alist nil t t nil nil))"
        ],
    )
}

fn atomic_chrome_all_functions_have_exact_call_interactive_documentation_and_source_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_all_functions_have_exact_call_interactive_documentation_and_source_contracts",
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
          '(atomic-chrome-get-websocket
            atomic-chrome-get-frame
            atomic-chrome-get-buffer-by-socket
            atomic-chrome-close-connection
            atomic-chrome-send-buffer-text
            atomic-chrome-set-major-mode
            atomic-chrome-show-edit-buffer
            atomic-chrome-create-buffer
            atomic-chrome-close-edit-buffer
            atomic-chrome-close-current-buffer
            atomic-chrome-update-buffer
            atomic-chrome-on-message
            atomic-chrome-on-close
            atomic-chrome-edit-mode
            atomic-chrome-turn-on-edit-mode
            global-atomic-chrome-edit-mode
            atomic-chrome-start-websocket-server
            atomic-chrome-start-httpd
            atomic-chrome-normalize-header
            atomic-chrome-httpd-parse-string
            atomic-chrome-httpd-process-filter
            atomic-chrome-httpd-send-response
            atomic-chrome-start-server
            atomic-chrome-stop-server))"##,
        expect![[
            r#"OK ((atomic-chrome-get-websocket t nil nil (buffer) "Look up websocket associated with buffer BUFFER.\nLooks in `atomic-chrome-buffer-table'." "atomic-chrome.el") (atomic-chrome-get-frame t nil nil (buffer) "Look up frame associated with buffer BUFFER.\nLooks in `atomic-chrome-buffer-table'." "atomic-chrome.el") (atomic-chrome-get-buffer-by-socket t nil nil (socket) "Look up buffer which is associated to the websocket SOCKET.\nLooks in `atomic-chrome-buffer-table'." "atomic-chrome.el") (atomic-chrome-close-connection t nil nil nil "Close client connection associated with current buffer." "atomic-chrome.el") (atomic-chrome-send-buffer-text t t (interactive nil) nil "Send request to update text with current buffer content." "atomic-chrome.el") (atomic-chrome-set-major-mode t nil nil (url) "Set major mode for editing buffer depending on URL.\n`atomic-chrome-url-major-mode-alist' can be used to select major mode.\nThe specified major mode is used if URL matches to one of the alist,\notherwise fallback to `atomic-chrome-default-major-mode'" "atomic-chrome.el") (atomic-chrome-show-edit-buffer t nil nil (buffer title) "Show editing buffer BUFFER.\nEither creates a frame with title TITLE, or raises the selected\nframe, depending on `atomic-chrome-buffer-open-style'." "atomic-chrome.el") (atomic-chrome-create-buffer t nil nil (socket url title text) "Create buffer associated with websocket specified by SOCKET.\nURL is used to determine the major mode of the buffer created,\nTITLE is used for the buffer name and TEXT is inserted to the buffer." "atomic-chrome.el") (atomic-chrome-close-edit-buffer t nil nil (buffer) "Close buffer BUFFER if it's one of Atomic Chrome edit buffers." "atomic-chrome.el") (atomic-chrome-close-current-buffer t t (interactive nil) nil "Close current buffer and connection from client." "atomic-chrome.el") (atomic-chrome-update-buffer t nil nil (socket text) "Update text on buffer associated with SOCKET to TEXT." "atomic-chrome.el") (atomic-chrome-on-message t nil nil (socket frame) "Handle data received from the websocket client specified by SOCKET.\nFRAME holds the raw data received." "atomic-chrome.el") (atomic-chrome-on-close t nil nil (socket) "Function to handle request from client to close websocket SOCKET." "atomic-chrome.el") (atomic-chrome-edit-mode t t (interactive #1=(list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) (&optional arg) "Minor mode enabled on buffers opened by Emacs Atomic Chrome server.\n\nThis is a minor mode.  If called interactively, toggle the\n`Atomic-Chrome-Edit mode' mode.  If the prefix argument is positive,\nenable the mode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable `atomic-chrome-edit-mode'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled.\n\n\\{atomic-chrome-edit-mode-map}" "atomic-chrome.el") (atomic-chrome-turn-on-edit-mode t nil nil nil "Turn on `atomic-chrome-edit-mode' if the buffer is an editing buffer." "atomic-chrome.el") (global-atomic-chrome-edit-mode t t (interactive #1#) (&optional arg) "Toggle Atomic-Chrome-Edit mode in many buffers.\nSpecifically, Atomic-Chrome-Edit mode is enabled in all buffers where\n`atomic-chrome-turn-on-edit-mode' would do it.\n\nWith prefix ARG, enable Global Atomic-Chrome-Edit mode if ARG is\npositive; otherwise, disable it.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.\nEnable the mode if ARG is nil, omitted, or is a positive number.\nDisable the mode if ARG is a negative number.\n\nSee `atomic-chrome-edit-mode' for more information on\nAtomic-Chrome-Edit mode." "atomic-chrome.el") (atomic-chrome-start-websocket-server t nil nil (port) "Create websocket server on port PORT." "atomic-chrome.el") (atomic-chrome-start-httpd t t (interactive nil) nil "Start the HTTP server for Ghost Text query." "atomic-chrome.el") (atomic-chrome-normalize-header t nil nil (header) "Destructively capitalize the components of HEADER." "atomic-chrome.el") (atomic-chrome-httpd-parse-string t nil nil (string) "Parse client http header STRING into alist." "atomic-chrome.el") (atomic-chrome-httpd-process-filter t nil nil (proc string) "Process filter of PROC which run each time client make a request.\nSTRING is the string process received." "atomic-chrome.el") (atomic-chrome-httpd-send-response t nil nil (proc) "Send an HTTP 200 OK response back to process PROC." "atomic-chrome.el") (atomic-chrome-start-server t t (interactive nil) nil "Start websocket server for atomic-chrome.\nFails silently if a server is already running." "atomic-chrome.el") (atomic-chrome-stop-server t t (interactive nil) nil "Stop websocket server for atomic-chrome." "atomic-chrome.el"))"#
        ]],
    )
}

fn atomic_chrome_customization_group_and_declared_variables_have_exact_contracts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atomic_chrome_customization_group_and_declared_variables_have_exact_contracts",
        r##"(list
          (get 'atomic-chrome 'custom-group)
          (documentation-property
           'atomic-chrome
           'group-documentation
           t)
          (get 'atomic-chrome 'custom-prefix)
          (mapcar
           (lambda (symbol)
             (let* ((standard-value
                     (copy-tree
                      (get symbol 'standard-value)))
                    (one-form
                     (and
                      (consp standard-value)
                      (null
                       (cdr standard-value))))
                    (evaluated
                     (and
                      one-form
                      (eval
                       (car standard-value)
                       t))))
               (list
                symbol
                (and
                 (custom-variable-p symbol)
                 t)
                (symbol-value symbol)
                one-form
                evaluated
                (equal
                 (symbol-value symbol)
                 evaluated)
                (get symbol 'custom-type)
                (get symbol 'custom-group)
                (documentation-property
                 symbol
                 'variable-documentation
                 t)
                (when-let
                    ((source
                      (symbol-file symbol 'defvar)))
                  (file-name-nondirectory
                   source)))))
           '(atomic-chrome-extension-type-list
             atomic-chrome-buffer-open-style
             atomic-chrome-buffer-frame-width
             atomic-chrome-buffer-frame-height
             atomic-chrome-server-ghost-text-port
             atomic-chrome-enable-auto-update
             atomic-chrome-enable-bidirectional-edit
             atomic-chrome-default-major-mode
             atomic-chrome-url-major-mode-alist
             atomic-chrome-edit-mode-hook
             atomic-chrome-edit-done-hook)))"##,
        expect![[
            r#"OK (((atomic-chrome-extension-type-list custom-variable) (atomic-chrome-buffer-open-style custom-variable) (atomic-chrome-buffer-frame-width custom-variable) (atomic-chrome-buffer-frame-height custom-variable) (atomic-chrome-server-ghost-text-port custom-variable) (atomic-chrome-enable-auto-update custom-variable) (atomic-chrome-enable-bidirectional-edit custom-variable) (atomic-chrome-default-major-mode custom-variable) (atomic-chrome-url-major-mode-alist custom-variable) (atomic-chrome-edit-mode-hook custom-variable) (atomic-chrome-edit-done-hook custom-variable) (global-atomic-chrome-edit-mode custom-variable)) "Edit browser text area with Emacs using Atomic Chrome or GhostText." "atomic-chrome-" ((atomic-chrome-extension-type-list t (atomic-chrome ghost-text) t (atomic-chrome ghost-text) t (repeat (choice (const :tag "Atomic Chrome" atomic-chrome) (const :tag "Ghost Text" ghost-text))) nil "List of browser extension type available." "atomic-chrome.el") (atomic-chrome-buffer-open-style t split t split t (choice (const :tag "Open buffer with full window" full) (const :tag "Open buffer with splitted window" split) (const :tag "Open buffer with new frame" frame)) nil "Specify the style to open new buffer for editing." "atomic-chrome.el") (atomic-chrome-buffer-frame-width t 80 t 80 t integer nil "Width of editing buffer frame." "atomic-chrome.el") (atomic-chrome-buffer-frame-height t 25 t 25 t integer nil "Height of editing buffer frame." "atomic-chrome.el") (atomic-chrome-server-ghost-text-port t 4001 t 4001 t integer nil "HTTP server port for Ghost Text." "atomic-chrome.el") (atomic-chrome-enable-auto-update t t t t t boolean nil "If non-nil, edit on Emacs is reflected to the browser instantly.\nIf nil, you need to type \"C-cC-s\" manually." "atomic-chrome.el") (atomic-chrome-enable-bidirectional-edit t t t t t boolean nil "If non-nil, you can edit both on the browser text area and Emacs.\nIf nil, edit on browser is ignored while editing on Emacs." "atomic-chrome.el") (atomic-chrome-default-major-mode t text-mode t text-mode t function nil "Default major mode for editing buffer." "atomic-chrome.el") (atomic-chrome-url-major-mode-alist t nil t nil t (alist :key-type (regexp :tag "regexp") :value-type (function :tag "major mode")) nil "Association list to select a major mode for a website.\nRelates URL (or, for GhostText, hostname) regular expressions to\ncorresponding major modes." "atomic-chrome.el") (atomic-chrome-edit-mode-hook t (atomic-chrome-edit-mode--set-explicitly) t nil nil hook nil "Customizable hook which run when the editing buffer is created." "atomic-chrome.el") (atomic-chrome-edit-done-hook t nil t nil t hook nil "Customizable hook which run when the editing buffer is closed." "atomic-chrome.el")))"#
        ]],
    )
}

fn atomic_chrome_internal_state_variables_and_mode_keymap_have_exact_contracts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atomic_chrome_internal_state_variables_and_mode_keymap_have_exact_contracts",
        r##"(list
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (boundp symbol)
              (cond
               ((eq
                 symbol
                 'atomic-chrome-buffer-table)
                (list
                 :hash-table
                 (hash-table-test
                  (symbol-value symbol))
                 (hash-table-count
                  (symbol-value symbol))))
               ((eq
                 symbol
                 'atomic-chrome-edit-mode-map)
                (list
                 :keymap
                 (keymapp
                  (symbol-value symbol))
                 (lookup-key
                  (symbol-value symbol)
                  (kbd "C-c C-s"))
                 (lookup-key
                  (symbol-value symbol)
                  (kbd "C-c C-c"))))
               (t
                (symbol-value symbol)))
              (default-boundp symbol)
              (special-variable-p symbol)
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
                 source))
              (copy-tree
               (get symbol 'standard-value))
              (local-variable-if-set-p symbol)))
           '(atomic-chrome-server-atomic-chrome
             atomic-chrome-server-ghost-text
             atomic-chrome-buffer-table
             atomic-chrome-edit-mode-map))
          (list
           (default-value
            'atomic-chrome-edit-mode)
           (local-variable-if-set-p
            'atomic-chrome-edit-mode)
           (assq
            'atomic-chrome-edit-mode
            minor-mode-alist)
           (assq
            'atomic-chrome-edit-mode
            minor-mode-map-alist)
           (default-value
            'global-atomic-chrome-edit-mode)
           (assq
            'global-atomic-chrome-edit-mode
            minor-mode-alist)))"##,
        expect![[
            r#"OK (((atomic-chrome-server-atomic-chrome t nil t t nil "Websocket server connection handle for Atomic Chrome." "atomic-chrome.el" nil nil) (atomic-chrome-server-ghost-text t nil t t nil "Websocket server connection handle for Ghost Text." "atomic-chrome.el" nil nil) (atomic-chrome-buffer-table t (:hash-table equal 0) t t nil "Hash table of editing buffer and its assciated data.\nEach element has a list consisting of (websocket, frame)." "atomic-chrome.el" nil nil) (atomic-chrome-edit-mode-map t (:keymap t atomic-chrome-send-buffer-text atomic-chrome-close-current-buffer) t t nil "Keymap for minor mode `atomic-chrome-edit-mode'." "atomic-chrome.el" nil nil)) (nil t (atomic-chrome-edit-mode " AtomicChrome") (atomic-chrome-edit-mode keymap (3 keymap (3 . atomic-chrome-close-current-buffer) (19 . atomic-chrome-send-buffer-text))) nil nil))"#
        ]],
    )
}

fn atomic_chrome_generated_autoload_preserves_feature_history_prefix_and_command_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_generated_autoload_preserves_feature_history_prefix_and_command_contracts",
        r##"(let* ((history
                (seq-find
                 (lambda (entry)
                   (and
                    (stringp
                     (car entry))
                    (string-suffix-p
                     "atomic-chrome-autoloads.el"
                     (car entry))))
                 load-history))
               (history-contract
                (mapcar
                 (lambda (event)
                   (list
                    (car event)
                    (cdr event)))
                 (seq-filter
                  (lambda (event)
                    (memq
                     (car-safe event)
                     '(defun provide)))
                  (cdr history)))))
          (list
           (featurep 'atomic-chrome-autoloads)
           (featurep 'atomic-chrome)
           history-contract
           (and
            (boundp 'definition-prefixes)
            (sort
             (delete-dups
              (copy-sequence
               (gethash
                "atomic-chrome-"
                definition-prefixes)))
             #'string<))
           (mapcar
            (lambda (symbol)
              (let ((definition
                     (and
                      (fboundp symbol)
                      (symbol-function
                       symbol))))
                (list
                 symbol
                 (autoloadp definition)
                 (and
                  (autoloadp definition)
                  (nth 1 definition))
                 (commandp symbol)
                 (help-function-arglist
                  symbol
                  t))))
            '(atomic-chrome-start-server
              atomic-chrome-stop-server))
           (mapcar
            (lambda (symbol)
              (list
               symbol
               (fboundp symbol)
               (boundp symbol)))
            '(atomic-chrome-on-message
              atomic-chrome-buffer-table
              atomic-chrome-server-ghost-text-port))))"##,
        expect![[
            r#"OK (t nil ((defun atomic-chrome-start-server) (defun atomic-chrome-stop-server) (provide atomic-chrome-autoloads)) ("atomic-chrome") ((atomic-chrome-start-server t "atomic-chrome" t "[Arg list not available until function definition is loaded.]") (atomic-chrome-stop-server t "atomic-chrome" t "[Arg list not available until function definition is loaded.]")) ((atomic-chrome-on-message nil nil) (atomic-chrome-buffer-table nil nil) (atomic-chrome-server-ghost-text-port nil nil)))"#
        ]],
    )
}

pub(super) fn registry_atomic_chrome_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atomic_chrome_descriptor_and_archive_sources_pin_exact_melpa_payload(),
        atomic_chrome_complete_prefixed_symbol_inventory_records_every_surface(),
        atomic_chrome_all_functions_have_exact_call_interactive_documentation_and_source_contracts(
        ),
        atomic_chrome_customization_group_and_declared_variables_have_exact_contracts(),
        atomic_chrome_internal_state_variables_and_mode_keymap_have_exact_contracts(),
    ]
}

pub(super) fn registry_atomic_chrome_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![atomic_chrome_generated_autoload_preserves_feature_history_prefix_and_command_contracts()]
}
