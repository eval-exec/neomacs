use expect_test::expect;

use super::ParityBatchCase;

fn auto_highlight_symbol_exact_package_descriptor_provenance_and_dependencies_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_exact_package_descriptor_provenance_and_dependencies_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-highlight-symbol
                                   package-alist)))
                                (extras
                                 (package-desc-extras descriptor)))
                           (list
                            (package-desc-name descriptor)
                            (package-version-join
                             (package-desc-version descriptor))
                            (package-desc-summary descriptor)
                            (package-desc-reqs descriptor)
                            (alist-get :commit extras)
                            (alist-get :revdesc extras)
                            (alist-get :url extras)
                            (file-name-nondirectory
                             (directory-file-name
                              (package-desc-dir descriptor)))))"##,
        expect![[
            r#"OK (auto-highlight-symbol "20260101.552" "Automatic highlighting current symbol minor mode." ((emacs (26 1)) (ht (2 3))) "e84da32e7cf1baefb0a9eef42a2fc842cf18f8b3" "e84da32e7cf1" "https://github.com/elp-revive/auto-highlight-symbol" "auto-highlight-symbol-20260101.552")"#
        ]],
    )
}

fn auto_highlight_symbol_installed_payload_inventory_and_exact_hashes_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_installed_payload_inventory_and_exact_hashes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-highlight-symbol
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor)))
                           (mapcar
                            (lambda (name)
                              (let ((file
                                     (expand-file-name
                                      name
                                      directory)))
                                (if
                                    (member
                                     name
                                     '("auto-highlight-symbol-pkg.el"
                                       "auto-highlight-symbol.el"))
                                    (list
                                     name
                                     :archive
                                     (file-attribute-size
                                      (file-attributes file))
                                     (with-temp-buffer
                                       (set-buffer-multibyte nil)
                                       (insert-file-contents-literally
                                        file)
                                       (secure-hash
                                        'sha256
                                        (current-buffer))))
                                  (list name :generated t))))
                            (sort
                             (directory-files
                              directory
                              nil
                              "\\`[^.]")
                             #'string<)))"##,
        expect![[
            r#"OK (("auto-highlight-symbol-autoloads.el" :generated t) ("auto-highlight-symbol-pkg.el" :archive 512 "ca26d132b79380307187c44f026a2fb5bbe519b0e7c8b97447cc9101c407b358") ("auto-highlight-symbol.el" :archive 58228 "52233bc912161cdc71d2d9977540dde163f125c3413a58903a1f8c929d9a049f") ("auto-highlight-symbol.elc" :generated t))"#
        ]],
    )
}

fn auto_highlight_symbol_complete_prefixed_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_complete_prefixed_symbol_inventory_matches",
        r##"(let (symbols)
                           (mapatoms
                            (lambda (symbol)
                              (let ((name
                                     (symbol-name symbol)))
                                (when
                                    (and
                                     (or
                                      (string-prefix-p
                                       "ahs-"
                                       name)
                                      (string-prefix-p
                                       "auto-highlight-symbol"
                                       name)
                                      (string-prefix-p
                                       "global-auto-highlight-symbol"
                                       name))
                                     (not
                                      (string-prefix-p
                                       "auto-highlight-symbol-test-"
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
                                     (macrop symbol)
                                     t)
                                    (local-variable-if-set-p
                                     symbol))
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
            "OK ((ahs--do-hl t nil nil nil nil) (ahs-add-overlay-face t nil nil t nil) (ahs-back-to-start t nil t nil nil) (ahs-backward t nil t nil nil) (ahs-backward-definition t nil t nil nil) (ahs-backward-p t nil nil nil nil) (ahs-called-interactively-p t nil nil nil nil) (ahs-case-fold-search nil t nil nil nil) (ahs-change-range t nil t nil nil) (ahs-change-range-internal t nil nil nil nil) (ahs-chrange-beginning-of-defun t nil t nil nil) (ahs-chrange-display t nil t nil nil) (ahs-chrange-whole-buffer t nil t nil nil) (ahs-clear t nil nil nil nil) (ahs-close-unnecessary-overlays t nil nil nil nil) (ahs-current-overlay nil t nil nil t) (ahs-current-overlay-window t nil nil nil nil) (ahs-current-plugin-prop t nil nil nil nil) (ahs-current-range nil t nil nil t) (ahs-decorate-if t nil nil t nil) (ahs-decorate-log nil t nil nil nil) (ahs-decorate-number t nil nil t nil) (ahs-decorated-current-plugin-name t nil nil nil nil) (ahs-default-range nil t nil nil nil) (ahs-default-symbol-regexp nil t nil nil nil) (ahs-definition-face nil t nil nil nil) (ahs-definition-face-list nil t nil nil nil) (ahs-definition-face-unfocused nil t nil nil nil) (ahs-definition-p t nil nil nil nil) (ahs-delete-overlays t nil nil nil nil) (ahs-disabled-commands nil t nil nil nil) (ahs-disabled-flags nil t nil nil nil) (ahs-disabled-minor-modes nil t nil nil nil) (ahs-display-stat t nil t nil nil) (ahs-dropdown-list-p t nil nil nil nil) (ahs-edit-mode t nil t nil nil) (ahs-edit-mode-condition-p t nil nil nil nil) (ahs-edit-mode-enable nil t nil nil t) (ahs-edit-mode-face nil t nil nil nil) (ahs-edit-mode-lighter-pair nil t nil nil nil) (ahs-edit-mode-off t nil nil nil nil) (ahs-edit-mode-off-hook nil t nil nil nil) (ahs-edit-mode-on t nil nil nil nil) (ahs-edit-mode-on-hook nil t nil nil nil) (ahs-edit-post-command-hook-function t nil nil nil nil) (ahs-enable-focus-hooks nil t nil nil nil) (ahs-exclude nil t nil nil nil) (ahs-face nil t nil nil nil) (ahs-face-check-include-overlay nil t nil nil nil) (ahs-face-p t nil nil nil nil) (ahs-face-unfocused nil t nil nil nil) (ahs-focus-in t nil nil nil nil) (ahs-focus-out t nil nil nil nil) (ahs-fontify t nil nil nil nil) (ahs-forward t nil t nil nil) (ahs-forward-definition t nil t nil nil) (ahs-forward-p t nil nil nil nil) (ahs-get-openable-overlays t nil nil nil nil) (ahs-get-overlay-face t nil nil nil nil) (ahs-get-plugin-prop t nil nil nil nil) (ahs-goto-web t nil t nil nil) (ahs-hidden-p t nil nil nil nil) (ahs-highlight t nil nil nil nil) (ahs-highlight-all-windows nil t nil nil nil) (ahs-highlight-current-symbol t nil nil nil nil) (ahs-highlight-now t nil t nil nil) (ahs-highlight-p t nil nil nil nil) (ahs-highlight-upon-window-switch nil t nil nil nil) (ahs-idle-function t nil nil nil nil) (ahs-idle-interval nil t nil nil nil) (ahs-idle-timer nil t nil nil nil) (ahs-include nil t nil nil nil) (ahs-inhibit-face-list nil t nil nil nil) (ahs-inhibit-modification nil t nil nil t) (ahs-inhibit-modification-commands nil t nil nil nil) (ahs-init t nil nil nil nil) (ahs-inside-display-p t nil nil nil nil) (ahs-inside-overlay-p t nil nil nil nil) (ahs-light-up t nil nil nil nil) (ahs-log t nil nil nil nil) (ahs-log-data nil t nil nil nil) (ahs-log-echo-area-only nil t nil nil nil) (ahs-log-format t nil nil t nil) (ahs-mode-line nil t nil nil t) (ahs-mode-maybe t nil nil nil nil) (ahs-mode-vers nil t nil nil nil) (ahs-modes nil t nil nil nil) (ahs-modification-hook t nil nil nil nil) (ahs-need-fontify nil t nil nil nil) (ahs-onekey-change t nil nil t nil) (ahs-onekey-edit t nil nil t nil) (ahs-onekey-edit-function t nil nil nil nil) (ahs-onekey-range-store nil t nil nil t) (ahs-open-invisible-overlay-temporary t nil nil nil nil) (ahs-open-necessary-overlay t nil nil nil nil) (ahs-opened-overlay-list nil t nil nil t) (ahs-overlay-list nil t nil nil t) (ahs-overlay-list-window t nil nil nil nil) (ahs-overlay-priority nil t nil nil nil) (ahs-overlays-in t nil nil nil nil) (ahs-plugin-ahs-bod t nil nil nil nil) (ahs-plugin-bod-end nil t nil nil nil) (ahs-plugin-bod-error t nil nil t nil) (ahs-plugin-bod-face nil t nil nil nil) (ahs-plugin-bod-function nil t nil nil nil) (ahs-plugin-bod-modes nil t nil nil nil) (ahs-plugin-bod-start nil t nil nil nil) (ahs-plugin-defalt-face nil nil nil nil nil) (ahs-plugin-default-face nil t nil nil nil) (ahs-plugin-default-face-unfocused nil t nil nil nil) (ahs-plugin-error-message t nil nil nil nil) (ahs-plugin-orignal-n2d t nil nil nil nil) (ahs-plugin-whole-buffer-face nil t nil nil nil) (ahs-prepare-highlight t nil nil nil nil) (ahs-range-beginning-of-defun nil t nil nil nil) (ahs-range-display nil t nil nil nil) (ahs-range-plugin-list nil t nil nil nil) (ahs-range-whole-buffer nil t nil nil nil) (ahs-regist-range-plugin t nil nil t nil) (ahs-remove-all-overlay t nil nil nil nil) (ahs-runnable-plugins t nil nil nil nil) (ahs-search-symbol t nil nil nil nil) (ahs-search-work nil t nil nil nil) (ahs-select t nil nil nil nil) (ahs-select-invisible nil t nil nil nil) (ahs-selected-window nil t nil nil nil) (ahs-set-idle-interval t nil t nil nil) (ahs-set-lighter t nil nil nil nil) (ahs-start-modification nil t nil nil t) (ahs-start-point nil t nil nil t) (ahs-start-point-p t nil nil nil nil) (ahs-start-timer t nil nil nil nil) (ahs-stat t nil nil nil nil) (ahs-stat-alert-p t nil nil nil nil) (ahs-stat-string t nil nil nil nil) (ahs-stop-timer t nil nil nil nil) (ahs-store-property t nil nil nil nil) (ahs-suppress-log nil t nil nil nil) (ahs-symbol nil nil nil nil nil) (ahs-symbol-modification t nil nil nil nil) (ahs-symbol-p t nil nil nil nil) (ahs-unfocus-all t nil t nil nil) (ahs-unhighlight t nil nil nil nil) (ahs-unhighlight-all t nil nil nil nil) (ahs-unhighlight-allowed-commands nil t nil nil nil) (ahs-valid-plugin-p t nil nil nil nil) (ahs-warning-face nil t nil nil nil) (ahs-web nil t nil nil nil) (ahs-window-map nil t nil nil nil) (auto-highlight-symbol nil nil nil nil nil) (auto-highlight-symbol-autoloads nil nil nil nil nil) (auto-highlight-symbol-mode t t t nil t) (auto-highlight-symbol-mode--set-explicitly t t nil nil t) (auto-highlight-symbol-mode--suppress-set-explicitly nil t nil nil nil) (auto-highlight-symbol-mode-hook nil t nil nil nil) (auto-highlight-symbol-mode-map nil t nil nil nil) (auto-highlight-symbol-mode-off-hook nil nil nil nil nil) (auto-highlight-symbol-mode-on-hook nil nil nil nil nil) (global-auto-highlight-symbol-mode t t t nil nil) (global-auto-highlight-symbol-mode-enable-in-buffer t nil nil nil nil) (global-auto-highlight-symbol-mode-hook nil t nil nil nil) (global-auto-highlight-symbol-mode-map nil nil nil nil nil) (global-auto-highlight-symbol-mode-off-hook nil nil nil nil nil) (global-auto-highlight-symbol-mode-on-hook nil nil nil nil nil) (global-auto-highlight-symbol-modes nil nil nil nil nil))"
        ],
    )
    .fresh_process()
}

fn auto_highlight_symbol_public_commands_arglists_interactivity_docs_and_origins_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_public_commands_arglists_interactivity_docs_and_origins_match",
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
                              (commandp symbol)
                              (documentation symbol t)
                              (when-let
                                  ((source
                                    (symbol-file
                                     symbol
                                     'defun)))
                                (file-name-nondirectory
                                 source))))
                           '(ahs-forward
                             ahs-backward
                             ahs-forward-definition
                             ahs-backward-definition
                             ahs-back-to-start
                             ahs-change-range
                             ahs-set-idle-interval
                             ahs-display-stat
                             ahs-highlight-now
                             ahs-unfocus-all
                             ahs-goto-web
                             ahs-edit-mode
                             auto-highlight-symbol-mode
                             global-auto-highlight-symbol-mode))"##,
        expect![[
            r#"OK ((ahs-forward nil t t "Select highlighted symbols forwardly." "auto-highlight-symbol.el") (ahs-backward nil t t "Select highlighted symbols backwardly." "auto-highlight-symbol.el") (ahs-forward-definition nil t t "Select highlighted symbols forwardly. only symbol definition." "auto-highlight-symbol.el") (ahs-backward-definition nil t t "Select highlighted symbols backwardly. only symbol definition." "auto-highlight-symbol.el") (ahs-back-to-start nil t t "Go back to the starting point.\n\nLimitation:\n  If you change plugin during highlights, starting point will be reset." "auto-highlight-symbol.el") (ahs-change-range (&optional range nomsg) t t "Current plugin change to `RANGE' plugin. `RANGE' defaults to next runnable\nplugin." "auto-highlight-symbol.el") (ahs-set-idle-interval (secs) t t "Set wait until highlighting symbol when emacs is idle." "auto-highlight-symbol.el") (ahs-display-stat nil t t "Display current status.\n\nDisplay current plugin name, number of matched symbols and the details.\n\nThe details are as follows:\n  1. Displayed symbols\n  2. Hidden symbols inside the display area\n  3. Symbols before the cursor\n  4. Symbols after the cursor\n\nThat's all." "auto-highlight-symbol.el") (ahs-highlight-now nil t t "Highlight NOW!!" "auto-highlight-symbol.el") (ahs-unfocus-all nil t t "Unfocus all windows." "auto-highlight-symbol.el") (ahs-goto-web nil t t "Go to official? web site." "auto-highlight-symbol.el") (ahs-edit-mode (arg &optional temporary) t t "Turn on edit mode. With a prefix argument, current plugin change to `whole\nbuffer' temporary." "auto-highlight-symbol.el") (auto-highlight-symbol-mode (&optional arg) t t "Toggle Auto Highlight Symbol Mode\n\nThis is a minor mode.  If called interactively, toggle the\n`Auto-Highlight-Symbol mode' mode.  If the prefix argument is positive,\nenable the mode, and if it is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable `auto-highlight-symbol-mode'.\n\nThe mode's hook is called both when the mode is enabled and when it is\ndisabled.\n\n\\{auto-highlight-symbol-mode-map}" "auto-highlight-symbol.el") (global-auto-highlight-symbol-mode (&optional arg) t t "Toggle Auto-Highlight-Symbol mode in many buffers.\nSpecifically, Auto-Highlight-Symbol mode is enabled in all buffers\nwhere `ahs-mode-maybe' would do it.\n\nWith prefix ARG, enable Global Auto-Highlight-Symbol mode if ARG is\npositive; otherwise, disable it.\n\nIf called from Lisp, toggle the mode if ARG is `toggle'.\nEnable the mode if ARG is nil, omitted, or is a positive number.\nDisable the mode if ARG is a negative number.\n\nSee `auto-highlight-symbol-mode' for more information on\nAuto-Highlight-Symbol mode." "auto-highlight-symbol.el"))"#
        ]],
    )
}

fn auto_highlight_symbol_custom_defaults_types_standard_values_and_locality_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_custom_defaults_types_standard_values_and_locality_match",
        r##"(mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (default-value symbol)
                              (get symbol 'custom-type)
                              (get symbol 'standard-value)
                              (local-variable-if-set-p
                               symbol)))
                           '(ahs-default-range
                             ahs-idle-interval
                             ahs-overlay-priority
                             ahs-highlight-all-windows
                             ahs-highlight-upon-window-switch
                             ahs-enable-focus-hooks
                             ahs-case-fold-search
                             ahs-include
                             ahs-exclude
                             ahs-face-check-include-overlay
                             ahs-select-invisible
                             ahs-disabled-commands
                             ahs-disabled-minor-modes
                             ahs-disabled-flags
                             ahs-plugin-bod-function
                             auto-highlight-symbol-mode
                             global-auto-highlight-symbol-mode))"##,
        expect![[
            r#"OK ((ahs-default-range ahs-range-display (choice (symbol :tag "Display area" ahs-range-display) (symbol :tag "Whole buffer" ahs-range-whole-buffer)) ('ahs-range-display) nil) (ahs-idle-interval 1.0 float (1.0) nil) (ahs-overlay-priority 1000 integer (1000) nil) (ahs-highlight-all-windows t boolean (t) nil) (ahs-highlight-upon-window-switch t boolean (t) nil) (ahs-enable-focus-hooks t boolean (t) nil) (ahs-case-fold-search t boolean (t) nil) (ahs-include "^[0-9A-Za-z/_.,:;*+=&%|$#@!^?-]+$" (choice (regexp :tag "Regexp" ahs-default-symbol-regexp) (symbol :tag "Function" function) (alist :tag "alist")) (ahs-default-symbol-regexp) nil) (ahs-exclude nil (choice (const :tag "All symbols can be highlighted" nil) (regexp :tag "Regexp" "") (symbol :tag "Function" function) (alist :tag "alist")) (nil) nil) (ahs-face-check-include-overlay nil boolean (nil) nil) (ahs-select-invisible immediate (choice (const :tag "Open hidden text only when necessary" immediate) (const :tag "Open hidden text temporary" temporary) (const :tag "Open hidden text permanently" open) (const :tag "Skip over all symbols in hidden text" skip)) ('immediate) nil) (ahs-disabled-commands nil list ('nil) nil) (ahs-disabled-minor-modes #1=(iedit-mode) list ('#1#) nil) (ahs-disabled-flags #2=(mark-active) list ('#2#) nil) (ahs-plugin-bod-function ahs-plugin-ahs-bod (choice (symbol :tag "Use built-in function" ahs-plugin-ahs-bod) (symbol :tag "Use original narrow-to-defun" ahs-plugin-orignal-n2d)) ('ahs-plugin-ahs-bod) nil) (auto-highlight-symbol-mode nil nil nil t) (global-auto-highlight-symbol-mode nil boolean (nil) nil))"#
        ]],
    )
    .fresh_process()
}

fn auto_highlight_symbol_faces_obsolete_alias_and_mode_map_bindings_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_faces_obsolete_alias_and_mode_map_bindings_match",
        r##"(list
                           (mapcar
                            (lambda (face)
                              (list
                               face
                               (facep face)
                               (get
                                face
                                'face-defface-spec)
                               (face-documentation face)))
                            '(ahs-face
                              ahs-definition-face
                              ahs-face-unfocused
                              ahs-definition-face-unfocused
                              ahs-plugin-default-face
                              ahs-plugin-default-face-unfocused
                              ahs-warning-face
                              ahs-plugin-whole-buffer-face
                              ahs-plugin-bod-face
                              ahs-edit-mode-face))
                           (get
                            'ahs-plugin-defalt-face
                            'face-alias)
                           (get
                            'ahs-plugin-defalt-face
                            'obsolete-face)
                           (mapcar
                            (lambda (key)
                              (list
                               key
                               (lookup-key
                                auto-highlight-symbol-mode-map
                                (kbd key))))
                            '("M-<left>"
                              "M-<right>"
                              "M-S-<left>"
                              "M-S-<right>"
                              "M--"
                              "C-x C-'"
                              "C-x C-a")))"##,
        expect![[
            r#"OK (((ahs-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "GhostWhite" :background "LightYellow4"))) "Highlight the symbol using this face (current).") (ahs-definition-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "moccasin" :background "CadetBlue"))) "Highlight the symbol definition using this face (current).") (ahs-face-unfocused [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "GhostWhite" :background "LightYellow4"))) "Highlight the symbol using this face (unfocused).") (ahs-definition-face-unfocused [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "moccasin" :background "CadetBlue"))) "Highlight the symbol definition using this face (unfocused).") (ahs-plugin-default-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "Black" :background "Orange1"))) "Face used in ‘display’ plugin (current).") (ahs-plugin-default-face-unfocused [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "Black" :background "Orange1"))) "Face used in ‘display’ plugin (unfocused).") (ahs-warning-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "Red" :bold t))) "Face for warning message.") (ahs-plugin-whole-buffer-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "Black" :background "GreenYellow"))) "Face used in ‘whole buffer’ plugin.") (ahs-plugin-bod-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "Black" :background "DodgerBlue"))) "Face used in ‘beginning of defun’ plugin.") (ahs-edit-mode-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "White" :background "Coral3"))) "Face used in edit mode.")) ahs-plugin-default-face "1.60" (("M-<left>" ahs-backward) ("M-<right>" ahs-forward) ("M-S-<left>" ahs-backward-definition) ("M-S-<right>" ahs-forward-definition) ("M--" ahs-back-to-start) ("C-x C-'" ahs-change-range) ("C-x C-a" ahs-edit-mode)))"#
        ]],
    )
}

fn auto_highlight_symbol_source_history_records_requires_plugins_definitions_and_feature()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_source_history_records_requires_plugins_definitions_and_feature",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-highlight-symbol.el"
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
                            ahs-range-plugin-list
                            (featurep
                             'auto-highlight-symbol)
                            (featurep 'ht)
                            (featurep 'dash)))"##,
        expect![[
            r#"OK ("auto-highlight-symbol.el" ((require . cl-lib) (require . easy-mmode) (require . subr-x) (require . ht) (defun . ahs-called-interactively-p) (defun . ahs-onekey-edit) (defun . ahs-onekey-change) (defun . ahs-decorate-if) (defun . ahs-log-format) (defun . ahs-log) (defun . ahs-overlays-in) (defun . ahs-current-overlay-window) (defun . ahs-overlay-list-window) (defun . ahs-regist-range-plugin) (defun . ahs-decorated-current-plugin-name) (defun . ahs-plugin-error-message) (defun . ahs-get-plugin-prop) (defun . ahs-current-plugin-prop) (defun . ahs-valid-plugin-p) (defun . ahs-runnable-plugins) (defun . ahs-change-range-internal) (defun . ahs-chrange-display) (defun . ahs-chrange-whole-buffer) (defun . ahs-plugin-bod-error) (defun . ahs-plugin-orignal-n2d) (defun . ahs-plugin-ahs-bod) (defun . ahs-chrange-beginning-of-defun) (defun . ahs-stop-timer) (defun . ahs-start-timer) (defun . ahs-idle-function) (defun . ahs--do-hl) (defun . ahs-add-overlay-face) (defun . ahs-highlight-p) (defun . ahs-symbol-p) (defun . ahs-dropdown-list-p) (defun . ahs-face-p) (defun . ahs-get-overlay-face) (defun . ahs-prepare-highlight) (defun . ahs-search-symbol) (defun . ahs-fontify) (defun . ahs-light-up) (defun . ahs-highlight) (defun . ahs-unhighlight) (defun . ahs-unhighlight-all) (defun . ahs-highlight-current-symbol) (defun . ahs-remove-all-overlay) (defun . ahs-modification-hook) (defun . ahs-edit-post-command-hook-function) (defun . ahs-symbol-modification) (defun . ahs-edit-mode-on) (defun . ahs-edit-mode-off) (defun . ahs-edit-mode-condition-p) (defun . ahs-onekey-edit-function) (defun . ahs-select) (defun . ahs-get-openable-overlays) (defun . ahs-close-unnecessary-overlays) (defun . ahs-open-necessary-overlay) (defun . ahs-open-invisible-overlay-temporary) (defun . ahs-store-property) (defun . ahs-forward-p) (defun . ahs-backward-p) (defun . ahs-definition-p) (defun . ahs-start-point-p) (defun . ahs-inside-overlay-p) (defun . ahs-inside-display-p) (defun . ahs-hidden-p) (defun . ahs-stat) (defun . ahs-stat-alert-p) (defun . ahs-decorate-number) (defun . ahs-stat-string) (defun . ahs-set-lighter) (defun . ahs-init) (defun . ahs-clear) (defun . ahs-mode-maybe) (defun . ahs-delete-overlays) (defun . ahs-forward) (defun . ahs-backward) (defun . ahs-forward-definition) (defun . ahs-backward-definition) (defun . ahs-back-to-start) (defun . ahs-change-range) (defun . ahs-set-idle-interval) (defun . ahs-display-stat) (defun . ahs-highlight-now) (defun . ahs-unfocus-all) (defun . ahs-goto-web) (defun . ahs-edit-mode) (defun . global-auto-highlight-symbol-mode) (defun . auto-highlight-symbol-mode--set-explicitly) (defun . global-auto-highlight-symbol-mode-enable-in-buffer) (defun . auto-highlight-symbol-mode) (defun . ahs-focus-in) (defun . ahs-focus-out) (provide . auto-highlight-symbol)) (ahs-range-beginning-of-defun ahs-range-whole-buffer ahs-range-display) t t t)"#
        ]],
    )
    .fresh_process()
}

fn auto_highlight_symbol_exact_ht_and_dash_dependency_versions_are_loaded() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_exact_ht_and_dash_dependency_versions_are_loaded",
        r##"(mapcar
                           (lambda (package)
                             (let ((descriptor
                                    (cadr
                                     (assq
                                      package
                                      package-alist))))
                               (list
                                package
                                (package-version-join
                                 (package-desc-version descriptor))
                                (package-desc-reqs descriptor)
                                (featurep package)
                                (file-name-nondirectory
                                 (locate-library
                                  (symbol-name package))))))
                           '(auto-highlight-symbol
                             ht
                             dash))"##,
        expect![[
            r#"OK ((auto-highlight-symbol "20260101.552" ((emacs (26 1)) (ht (2 3))) t "auto-highlight-symbol.el") (ht "20230703.558" ((dash (2 12 0))) t "ht.el") (dash "20260221.1346" ((emacs (24))) t "dash.el"))"#
        ]],
    )
}

fn auto_highlight_symbol_generated_autoload_contract_and_real_activation_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_highlight_symbol_generated_autoload_contract_and_real_activation_match",
        r##"(let* ((history
                                 (seq-find
                                  (lambda (entry)
                                    (and
                                     (stringp
                                      (car entry))
                                     (string-suffix-p
                                      "auto-highlight-symbol-autoloads.el"
                                      (car entry))))
                                  load-history))
                                (events
                                 (seq-filter
                                  (lambda (event)
                                    (memq
                                     (car-safe event)
                                     '(defun provide)))
                                  (cdr history)))
                                (local-definition
                                 (symbol-function
                                  'auto-highlight-symbol-mode))
                                (global-definition
                                 (symbol-function
                                  'global-auto-highlight-symbol-mode)))
                           (with-temp-buffer
                             (emacs-lisp-mode)
                             (auto-highlight-symbol-mode 1)
                             (list
                              events
                              (autoloadp local-definition)
                              (autoloadp global-definition)
                              (featurep
                               'auto-highlight-symbol)
                              (featurep 'ht)
                              auto-highlight-symbol-mode
                              ahs-current-range
                              ahs-mode-line
                              (memq
                               'ahs-start-timer
                               post-command-hook)
                              (memq
                               'ahs-start-timer
                               after-change-functions))))"##,
        expect![[
            r#"OK (((defun . global-auto-highlight-symbol-mode) (defun . auto-highlight-symbol-mode) (provide . auto-highlight-symbol-autoloads)) t t t t t ((name . "display area") (lighter . "HS") (start . window-start) (end . window-end)) " HS" (ahs-start-timer eldoc-schedule-timer t) (ahs-start-timer t))"#
        ]],
    )
}

pub(super) fn registry_auto_highlight_symbol_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_highlight_symbol_exact_package_descriptor_provenance_and_dependencies_match(),
        auto_highlight_symbol_installed_payload_inventory_and_exact_hashes_match(),
        auto_highlight_symbol_complete_prefixed_symbol_inventory_matches(),
        auto_highlight_symbol_public_commands_arglists_interactivity_docs_and_origins_match(),
        auto_highlight_symbol_custom_defaults_types_standard_values_and_locality_match(),
        auto_highlight_symbol_faces_obsolete_alias_and_mode_map_bindings_match(),
        auto_highlight_symbol_source_history_records_requires_plugins_definitions_and_feature(),
        auto_highlight_symbol_exact_ht_and_dash_dependency_versions_are_loaded(),
    ]
}

pub(super) fn registry_auto_highlight_symbol_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_highlight_symbol_generated_autoload_contract_and_real_activation_match()]
}
