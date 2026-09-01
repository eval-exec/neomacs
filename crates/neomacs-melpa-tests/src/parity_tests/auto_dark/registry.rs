use expect_test::expect;

use super::ParityBatchCase;

fn auto_dark_exact_descriptor_activation_and_payload_bytes_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_exact_descriptor_activation_and_payload_bytes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-dark
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor)))
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-kind descriptor)
           (package-desc-reqs descriptor)
           (package-desc-extras descriptor)
           (featurep 'auto-dark)
           (package-installed-p
            'auto-dark
            '(20260313 2356))
           (mapcar
            (lambda (file)
              (let ((path
                     (expand-file-name
                      file
                      directory)))
                (list
                 file
                 (file-attribute-size
                  (file-attributes path))
                 (with-temp-buffer
                   (insert-file-contents-literally
                    path)
                   (secure-hash
                    'sha256
                    (current-buffer))))))
            '("auto-dark-pkg.el"
              "auto-dark.el"))))"##,
        expect![[
            r#"OK (auto-dark "20260313.2356" "Automatically set the dark-mode theme based on system status." nil ((emacs (24 4))) ((:maintainers ("Tim Harper" . "timcharperatgmaildotcom") ("Vincent Zhang" . "seagle0128@gmail.com") ("Jonathan Arnett" . "jonathan.arnett@protonmail.com") ("Greg Pfeil" . "greg@technomadic.org")) (:authors ("Tim Harper" . "timcharperatgmaildotcom") ("Vincent Zhang" . "seagle0128@gmail.com") ("Jonathan Arnett" . "jonathan.arnett@protonmail.com") ("Greg Pfeil" . "greg@technomadic.org")) (:keywords "macos" "windows" "linux" "themes" "tools" "faces") (:revdesc . "6d1e8d2fc493") (:commit . "6d1e8d2fc493dccbf05c9191611805c7e7881c70") (:url . "https://github.com/LionyxML/auto-dark-emacs")) t t (("auto-dark-pkg.el" 865 "e12146a1d981b40f395a360091f055b872966effc38dc39a14ae05c7450da981") ("auto-dark.el" 20097 "382101f5b609e30bc95016040cba9b50e397e51fadb1aff7419feb0277311070")))"#
        ]],
    )
}

fn auto_dark_complete_prefixed_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_complete_prefixed_symbol_inventory_matches",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name
                    (symbol-name symbol)))
               (when
                   (and
                    (string-prefix-p
                     "auto-dark"
                     name)
                    (not
                     (string-prefix-p
                      "auto-dark-test-"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (and
                    (macrop symbol)
                    t)
                   (boundp symbol)
                   (and
                    (custom-variable-p symbol)
                    t)
                   (and
                    (commandp symbol)
                    t)
                   (when-let
                       ((source
                         (or
                          (symbol-file symbol 'defun)
                          (symbol-file symbol 'defvar))))
                     (file-name-nondirectory
                      source)))
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
            r#"OK ((auto-dark nil nil nil nil nil nil) (auto-dark--check-and-set-dark-mode t nil nil nil nil "auto-dark.el") (auto-dark--current-mode-applescript t nil nil nil nil "auto-dark.el") (auto-dark--current-mode-dbus t nil nil nil nil "auto-dark.el") (auto-dark--current-system-mode t nil nil nil nil "auto-dark.el") (auto-dark--dbus-listener-object nil nil t nil nil "auto-dark.el") (auto-dark--determine-detection-method t nil nil nil nil "auto-dark.el") (auto-dark--enable-themes t nil nil nil t "auto-dark.el") (auto-dark--initialized-p t nil nil nil nil "auto-dark.el") (auto-dark--is-dark-mode-mac t nil nil nil nil "auto-dark.el") (auto-dark--is-dark-mode-ns t nil nil nil nil "auto-dark.el") (auto-dark--is-dark-mode-osascript t nil nil nil nil "auto-dark.el") (auto-dark--is-dark-mode-powershell t nil nil nil nil "auto-dark.el") (auto-dark--is-dark-mode-termux t nil nil nil nil "auto-dark.el") (auto-dark--is-dark-mode-winreg t nil nil nil nil "auto-dark.el") (auto-dark--last-dark-mode-state nil nil t nil nil "auto-dark.el") (auto-dark--register-change-listener t nil nil nil nil "auto-dark.el") (auto-dark--register-dbus-listener t nil nil nil nil "auto-dark.el") (auto-dark--set-theme t nil nil nil nil "auto-dark.el") (auto-dark--themes-for-mode t nil nil nil nil "auto-dark.el") (auto-dark--timer nil nil t nil nil "auto-dark.el") (auto-dark--unregister-change-listener t nil nil nil nil "auto-dark.el") (auto-dark--unregister-dbus-listener t nil nil nil nil "auto-dark.el") (auto-dark--update-frame-backgrounds t nil nil nil nil "auto-dark.el") (auto-dark--use-dbus t nil nil nil nil "auto-dark.el") (auto-dark--use-mac-system-appearance t nil nil nil nil "auto-dark.el") (auto-dark--use-ns-system-appearance t nil nil nil nil "auto-dark.el") (auto-dark-allow-osascript nil nil t t nil "auto-dark.el") (auto-dark-allow-powershell nil nil t t nil "auto-dark.el") (auto-dark-autoloads nil nil nil nil nil nil) (auto-dark-dark-mode-hook nil nil t nil nil "auto-dark.el") (auto-dark-detection-method nil nil t t nil "auto-dark.el") (auto-dark-light-mode-hook nil nil t nil nil "auto-dark.el") (auto-dark-mode t nil t t t "auto-dark.el") (auto-dark-mode-hook nil nil t t nil "auto-dark.el") (auto-dark-mode-map nil nil nil nil nil nil) (auto-dark-mode-off-hook nil nil nil nil nil nil) (auto-dark-mode-on-hook nil nil nil nil nil nil) (auto-dark-polling-interval-seconds nil nil t t nil "auto-dark.el") (auto-dark-start-timer t nil nil nil nil "auto-dark.el") (auto-dark-stop-timer t nil nil nil nil "auto-dark.el") (auto-dark-themes nil nil t t nil "auto-dark.el") (auto-dark-toggle-appearance t nil nil nil t "auto-dark.el"))"#
        ]],
    )
    .fresh_process()
}

fn auto_dark_every_callable_arglist_command_doc_and_source_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_every_callable_arglist_command_doc_and_source_match",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (and
              (commandp symbol)
              t)
             (interactive-form symbol)
             (copy-tree
              (help-function-arglist
               symbol
               t))
             (documentation symbol t)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          '(auto-dark--current-mode-applescript
            auto-dark--is-dark-mode-ns
            auto-dark--is-dark-mode-mac
            auto-dark--is-dark-mode-osascript
            auto-dark--current-mode-dbus
            auto-dark--is-dark-mode-powershell
            auto-dark--is-dark-mode-winreg
            auto-dark--is-dark-mode-termux
            auto-dark--current-system-mode
            auto-dark--initialized-p
            auto-dark--check-and-set-dark-mode
            auto-dark--update-frame-backgrounds
            auto-dark--enable-themes
            auto-dark-toggle-appearance
            auto-dark--set-theme
            auto-dark-start-timer
            auto-dark-stop-timer
            auto-dark--register-dbus-listener
            auto-dark--unregister-dbus-listener
            auto-dark--register-change-listener
            auto-dark--unregister-change-listener
            auto-dark--use-ns-system-appearance
            auto-dark--use-mac-system-appearance
            auto-dark--use-dbus
            auto-dark--determine-detection-method
            auto-dark-mode
            auto-dark--themes-for-mode))"##,
        expect![[
            r#"OK ((auto-dark--current-mode-applescript nil nil nil "Invoke AppleScript using Emacs built-in AppleScript support.\nIn order to check if dark mode is enabled.  Return true if it is." "auto-dark.el") (auto-dark--is-dark-mode-ns nil nil nil "Check if dark mode is enabled using `ns-do-applescript'." "auto-dark.el") (auto-dark--is-dark-mode-mac nil nil nil "Check if dark mode is enabled using `mac-do-applescript'." "auto-dark.el") (auto-dark--is-dark-mode-osascript nil nil nil "Invoke applescript using Emacs using external shell command;\nthis is less efficient, but works for non-GUI Emacs." "auto-dark.el") (auto-dark--current-mode-dbus nil nil nil "Use Emacs built-in D-Bus function to determine if dark theme is enabled." "auto-dark.el") (auto-dark--is-dark-mode-powershell nil nil nil "Invoke PowerShell and detect dark mode.\nCompatible with both bash pure output and zsh mixed output." "auto-dark.el") (auto-dark--is-dark-mode-winreg nil nil nil "Use Emacs built-in Windows Registry function.\nIn order to determine if dark theme is enabled." "auto-dark.el") (auto-dark--is-dark-mode-termux nil nil nil "Use Termux way to determine if dark theme is enabled.\nref: https://github.com/termux/termux-api/issues/425." "auto-dark.el") (auto-dark--current-system-mode nil nil nil "Return our best guess of the mode the system is in.\nIt can be dark, light, or nil." "auto-dark.el") (auto-dark--initialized-p nil nil nil "Check whether initialization is far enough along to change themes." "auto-dark.el") (auto-dark--check-and-set-dark-mode nil nil nil "Set the theme according to the OS's dark mode state.\nIn order to prevent flickering, we only set the theme if we haven't\nalready set the theme for the current dark mode state." "auto-dark.el") (auto-dark--update-frame-backgrounds nil nil (appearance) "Set the `frame-background-mode' for all frames to APPEARANCE." "auto-dark.el") (auto-dark--enable-themes t (interactive nil) (&optional themes) "Re-enable THEMES, which defaults to ‘custom-enabled-themes’.\nThis will load themes if necessary." "auto-dark.el") (auto-dark-toggle-appearance t (interactive nil) nil "Switch between light and dark mode.\nIf `auto-dark-detection-method' is nil, this will persist until the next time\nthis is called. Otherwise, it could switch to the system appearance at any\ntime." "auto-dark.el") (auto-dark--set-theme nil nil (appearance) "Set light/dark theme Argument APPEARANCE should be light or dark." "auto-dark.el") (auto-dark-start-timer nil nil nil "Start auto-dark timer." "auto-dark.el") (auto-dark-stop-timer nil nil nil "Stop auto-dark timer." "auto-dark.el") (auto-dark--register-dbus-listener nil nil nil "Register a callback function with D-Bus.\nAsk D-Bus to send us a signal on theme change and add a callback\nto change the theme." "auto-dark.el") (auto-dark--unregister-dbus-listener nil nil nil "Unregister our callback function with D-Bus.\nRemove theme change callback registered with D-Bus." "auto-dark.el") (auto-dark--register-change-listener nil nil nil "Register a listener to listen for the system theme to change." "auto-dark.el") (auto-dark--unregister-change-listener nil nil nil "Remove an existing listener for the system theme." "auto-dark.el") (auto-dark--use-ns-system-appearance nil nil nil "Determine whether we should use the ns-system-appearance-* functions." "auto-dark.el") (auto-dark--use-mac-system-appearance nil nil nil "Determine whether we should use the `mac-effective-appearance-change-hook'." "auto-dark.el") (auto-dark--use-dbus nil nil nil "Determine whether we should use the dbus-* functions." "auto-dark.el") (auto-dark--determine-detection-method nil nil nil "Determine which theme detection method auto-dark should use." "auto-dark.el") (auto-dark-mode t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) (&optional arg) "Toggle `auto-dark-mode' on or off.\n\nThis is a global minor mode.  If called interactively, toggle the\n`Auto-Dark mode' mode.  If the prefix argument is positive, enable the\nmode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate `(default-value \\='auto-dark-mode)'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled." "auto-dark.el") (auto-dark--themes-for-mode nil nil (mode) "Return the set of themes to be used in MODE.\nMODE should be light or dark. If none of the Auto-Dark theme variables are set,\nthis returns nil, which means that `custom-enabled-themes' will be used as the\ntheme list." "auto-dark.el"))"#
        ]],
    )
}

fn auto_dark_custom_group_and_every_option_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_custom_group_and_every_option_contract_match",
        r##"(list
          (list
           (get 'auto-dark 'custom-group)
           (get
            'auto-dark
            'group-documentation)
           (get
            'auto-dark
            'custom-prefix))
          (mapcar
           (lambda (symbol)
             (let ((standard-value
                    (copy-tree
                     (get symbol 'standard-value))))
               (list
                symbol
                (and
                 (custom-variable-p symbol)
                 t)
                (symbol-value symbol)
                (default-value symbol)
                standard-value
                (copy-tree
                 (get symbol 'custom-type))
                (copy-tree
                 (get symbol 'custom-options))
                (get symbol 'custom-group)
                (get symbol 'custom-version)
                (and
                 (get symbol 'custom-set)
                 t)
                (documentation-property
                 symbol
                 'variable-documentation
                 t)
                (special-variable-p symbol)
                (local-variable-if-set-p
                 symbol)
                (file-name-nondirectory
                 (symbol-file symbol 'defvar)))))
           '(auto-dark-polling-interval-seconds
             auto-dark-allow-osascript
             auto-dark-allow-powershell
             auto-dark-detection-method
             auto-dark-themes)))"##,
        expect![[
            r#"OK ((((auto-dark-polling-interval-seconds custom-variable) (auto-dark-allow-osascript custom-variable) (auto-dark-allow-powershell custom-variable) (auto-dark-detection-method custom-variable) (auto-dark-mode custom-variable) (auto-dark-themes custom-variable)) "Automatically changes Emacs theme acording to MacOS/Windows dark-mode status." "auto-dark-*") ((auto-dark-polling-interval-seconds t 5 5 ((funcall #'#[nil (5) #1=(t)])) integer nil nil nil nil "The number of seconds between which to poll for dark mode state.\nEmacs must be restarted for this value to take effect." t nil "auto-dark.el") (auto-dark-allow-osascript t nil nil ((funcall #'#[nil (nil) #1#])) boolean nil nil nil nil "Whether to allow function `auto-dark-mode' to shell out to osascript:\nto check dark-mode state, if `ns-do-applescript' or `mac-do-applescript'\nis not available." t nil "auto-dark.el") (auto-dark-allow-powershell t nil nil ((funcall #'#[nil (nil) #1#])) boolean nil nil nil nil "Whether to allow function `auto-dark-mode' to shell out to powershell:\nto check dark-mode state." t nil "auto-dark.el") (auto-dark-detection-method t nil nil ((funcall #'#[nil (nil) #1#])) symbol (applescript osascript dbus powershell winreg termux) nil nil nil "The method auto-dark should use to detect the system theme.\n\nDefaults to nil and will be populated through feature detection\nif left as such.  Only change this value if you know what you're\ndoing!" t nil "auto-dark.el") (auto-dark-themes t nil nil ((funcall #'#[nil (nil) #1#])) (choice (const :tag "Use custom-enabled-themes" nil) (list :tag "Use distinct dark & light lists" (repeat :tag "Dark" symbol) (repeat :tag "Light" symbol))) nil nil "0.13" t "The themes to enable for dark and light modes.\nThe default is to use the themes in `custom-enabled-themes', but that only works\nif the themes are aware of `frame-background-mode', which many aren’t.\n\nIf your themes aren’t aware of `frame-background-mode' (or you just prefer\ndifferent themes for dark and light modes), you can set explicit lists of themes\nfor each mode. Like with `custom-enabled-themes', the earlier themes in the list\nhave higher precedence.\n\nOne other thing to be aware of is that when you first load a theme, you may be\nprompted to acknowledge that the theme can run arbitrary Lisp code.\nAcknowledging this and then allowing Emacs to treat the theme as safe in future\nsessions will silence the prompt (for that particular theme). If you would just\nprefer to ignore this warning for all themes, you can set `custom-safe-themes'\nto t." t nil "auto-dark.el")))"#
        ]],
    )
}

fn auto_dark_mode_hook_timer_listener_and_state_variable_metadata_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_mode_hook_timer_listener_and_state_variable_metadata_match",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (boundp symbol)
             (and
              (boundp symbol)
              (symbol-value symbol))
             (documentation-property
              symbol
              'variable-documentation
              t)
             (get symbol 'permanent-local)
             (get symbol 'risky-local-variable)
             (when-let
                 ((source
                   (symbol-file symbol 'defvar)))
               (file-name-nondirectory
                source))))
          '(auto-dark-mode
            auto-dark-mode-hook
            auto-dark--last-dark-mode-state
            auto-dark--dbus-listener-object
            auto-dark--timer
            auto-dark-dark-mode-hook
            auto-dark-light-mode-hook))"##,
        expect![[
            r#"OK ((auto-dark-mode t nil "Non-nil if Auto-Dark mode is enabled.\nSee the `auto-dark-mode' command\nfor a description of this minor mode.\nSetting this variable directly does not take effect;\neither customize it (see the info node `Easy Customization')\nor call the function `auto-dark-mode'." nil nil "auto-dark.el") (auto-dark-mode-hook t nil "Hook run after entering or leaving `auto-dark-mode'.\nNo problems result if this variable is not bound.\n`add-hook' automatically binds it.  (This is true for all hook variables.)" nil nil "auto-dark.el") (auto-dark--last-dark-mode-state t unknown nil nil nil "auto-dark.el") (auto-dark--dbus-listener-object t nil nil nil nil "auto-dark.el") (auto-dark--timer t nil nil nil nil "auto-dark.el") (auto-dark-dark-mode-hook t nil "List of hooks to run after dark mode is loaded." nil nil "auto-dark.el") (auto-dark-light-mode-hook t nil "List of hooks to run after light mode is loaded." nil nil "auto-dark.el"))"#
        ]],
    )
    .fresh_process()
}

fn auto_dark_source_load_history_records_complete_definition_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_source_load_history_records_complete_definition_order",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-dark.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(require
                                       defun
                                       provide)))
                                  (cdr history))))
          (list
           (file-name-nondirectory
            (car history))
           events
           (featurep 'dbus)
           (featurep 'auto-dark)))"##,
        expect![[
            r#"OK ("auto-dark.el" ((require . dbus) (defun . auto-dark--current-mode-applescript) (defun . auto-dark--is-dark-mode-ns) (defun . auto-dark--is-dark-mode-mac) (defun . auto-dark--is-dark-mode-osascript) (defun . auto-dark--current-mode-dbus) (defun . auto-dark--is-dark-mode-powershell) (defun . auto-dark--is-dark-mode-winreg) (defun . auto-dark--is-dark-mode-termux) (defun . auto-dark--current-system-mode) (defun . auto-dark--initialized-p) (defun . auto-dark--check-and-set-dark-mode) (defun . auto-dark--update-frame-backgrounds) (defun . auto-dark--enable-themes) (defun . auto-dark-toggle-appearance) (defun . auto-dark--set-theme) (defun . auto-dark-start-timer) (defun . auto-dark-stop-timer) (defun . auto-dark--register-dbus-listener) (defun . auto-dark--unregister-dbus-listener) (defun . auto-dark--register-change-listener) (defun . auto-dark--unregister-change-listener) (defun . auto-dark--use-ns-system-appearance) (defun . auto-dark--use-mac-system-appearance) (defun . auto-dark--use-dbus) (defun . auto-dark--determine-detection-method) (defun . auto-dark-mode) (defun . auto-dark--themes-for-mode) (provide . auto-dark)) t t)"#
        ]],
    )
}

fn auto_dark_source_reload_preserves_custom_values_and_mode_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_source_reload_preserves_custom_values_and_mode_state",
        r##"(let ((source
                                (getenv
                                 "NEOMACS_PACKAGE_SOURCE"))
                               (auto-dark-polling-interval-seconds
                                17)
                               (auto-dark-allow-osascript t)
                               (auto-dark-allow-powershell t)
                               (auto-dark-detection-method
                                'manual)
                               (auto-dark-themes
                                '((wombat)
                                  (leuven)))
                               (auto-dark--last-dark-mode-state
                                'dark))
          (cl-letf
              (((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                #'ignore)
               ((symbol-function
                 'auto-dark--register-change-listener)
                #'ignore)
               ((symbol-function
                 'auto-dark--unregister-change-listener)
                #'ignore))
            (auto-dark-mode 1)
            (load source nil t t)
            (list
             auto-dark-polling-interval-seconds
             auto-dark-allow-osascript
             auto-dark-allow-powershell
             auto-dark-detection-method
             auto-dark-themes
             auto-dark--last-dark-mode-state
             auto-dark-mode
             (featurep 'auto-dark))))"##,
        expect!["OK (17 t t manual nil dark t t)"],
    )
}

fn auto_dark_generated_autoload_exposes_only_global_mode_before_activation() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_generated_autoload_exposes_only_global_mode_before_activation",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-dark-autoloads.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(defun provide)))
                                  (cdr history)))
                                (definition
                                 (symbol-function
                                  'auto-dark-mode)))
          (list
           (featurep
            'auto-dark-autoloads)
           (featurep 'auto-dark)
           events
           (autoloadp definition)
           (nth 1 definition)
           (nth 2 definition)
           (nth 4 definition)
           (commandp 'auto-dark-mode)
           (fboundp
            'auto-dark-toggle-appearance)
           (boundp 'auto-dark-themes)
           (and
            (boundp
             'definition-prefixes)
            (gethash
             "auto-dark"
             definition-prefixes))))"##,
        expect![[
            r#"OK (t nil ((defun . auto-dark-mode) (provide . auto-dark-autoloads)) t "auto-dark" "Toggle `auto-dark-mode' on or off.\n\nThis is a global minor mode.  If called interactively, toggle the\n`Auto-Dark mode' mode.  If the prefix argument is positive, enable the\nmode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate `(default-value \\='auto-dark-mode)'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled.\n\n(fn &optional ARG)" nil t nil nil nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn registry_auto_dark_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dark_exact_descriptor_activation_and_payload_bytes_match(),
        auto_dark_complete_prefixed_symbol_inventory_matches(),
        auto_dark_every_callable_arglist_command_doc_and_source_match(),
        auto_dark_custom_group_and_every_option_contract_match(),
        auto_dark_mode_hook_timer_listener_and_state_variable_metadata_match(),
        auto_dark_source_load_history_records_complete_definition_order(),
        auto_dark_source_reload_preserves_custom_values_and_mode_state(),
    ]
}

pub(super) fn registry_auto_dark_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_dark_generated_autoload_exposes_only_global_mode_before_activation()]
}
