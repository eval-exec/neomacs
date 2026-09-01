use expect_test::expect;

use super::ParityBatchCase;

fn aurel_descriptor_and_archive_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_descriptor_and_archive_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'aurel
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (mapcar
                 (lambda (name)
                   (expand-file-name
                    name
                    directory))
                 '("aurel-pkg.el"
                   "aurel.el"))))
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
                (insert-file-contents-literally
                 file)
                (secure-hash
                 'sha256
                 (current-buffer)))))
           sources)))"##,
        expect![[
            r#"OK ((aurel "20260429.458" "Search, get info, vote for and download AUR packages." ((emacs (29 1)) (bui (1 1 0))) ((:maintainers ("Alex Kost" . "alezost@gmail.com")) (:authors ("Alex Kost" . "alezost@gmail.com")) (:keywords "tools") (:revdesc . "c571cc44ea3b") (:commit . "c571cc44ea3b9aa96399056bff22919efffbbb06") (:url . "https://github.com/alezost/aurel"))) (("aurel-pkg.el" 438 "c2a58a99cd0662ae18dfc69e7f22e36b5f9f1e358921889924542d7fc7cad54b") ("aurel.el" 70342 "f0d13df4c21b3181d16a10bca758b5c6afc050c63bcb219f23657d4ef60a7944")))"#
        ]],
    )
}

fn aurel_exact_bui_dependency_pin_is_active_with_both_features_loaded() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_exact_bui_dependency_pin_is_active_with_both_features_loaded",
        r##"(let ((aurel-descriptor
                (package--get-activatable-pkg
                 'aurel))
               (bui-descriptor
                (package--get-activatable-pkg
                 'bui)))
         (list
          package-load-list
          (mapcar
           (lambda (entry)
             (pcase-let
                 ((`(,name
                     ,descriptor
                     ,expected-version)
                   entry))
               (list
                name
                (package-version-join
                 (package-desc-version
                  descriptor))
                (and
                 (memq
                  name
                  package-activated-list)
                 t)
                (equal
                 (package-version-join
                  (package-desc-version
                   descriptor))
                 expected-version)
                (file-name-nondirectory
                 (directory-file-name
                  (package-desc-dir
                   descriptor))))))
           `((aurel
              ,aurel-descriptor
              "20260429.458")
             (bui
              ,bui-descriptor
              "20260502.730")))
          (featurep 'aurel)
          (featurep 'bui)
          (derived-mode-p
           'aurel-list-mode)
          (fboundp
           'aurel-info-mode)))"##,
        expect![[
            r#"OK ((all (aurel "20260429.458") (bui "20260502.730")) ((aurel "20260429.458" t t "aurel-20260429.458") (bui "20260502.730" t t "bui-20260502.730")) t t nil t)"#
        ]],
    )
}

fn aurel_public_commands_have_exact_interactive_and_argument_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_public_commands_have_exact_interactive_and_argument_contracts",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist
             symbol
             t)
            (file-name-nondirectory
             (symbol-file
              symbol
              'defun))))
         '(aurel-aur-login-maybe
           aurel-package-info
           aurel-package-search
           aurel-package-search-by-name
           aurel-maintainer-search
           aurel-installed-packages
           aurel-enable-filter
           aurel-filter-maintained
           aurel-filter-unmaintained
           aurel-filter-outdated
           aurel-filter-not-outdated
           aurel-filter-same-versions
           aurel-filter-different-versions
           aurel-filter-match-regexp
           aurel-filter-not-match-regexp
           aurel-list-download-package
           aurel-info-download-package
           aurel-info-vote-unvote
           aurel-info-subscribe-unsubscribe))"##,
        expect![[
            r#"OK ((aurel-aur-login-maybe t (interactive "P") (&optional force noerror) "aurel.el") (aurel-package-info t (interactive (list (read-string "Name: " nil 'aurel-package-info-history))) (name) "aurel.el") (aurel-package-search t (interactive (list (read-string "Search by name/description: " nil 'aurel-package-search-history))) (string) "aurel.el") (aurel-package-search-by-name t (interactive (list (read-string "Search by name: " nil 'aurel-package-search-history))) (string) "aurel.el") (aurel-maintainer-search t (interactive (list (read-string "Search by maintainer: " nil 'aurel-maintainer-search-history))) (name) "aurel.el") (aurel-installed-packages t (interactive nil) nil "aurel.el") (aurel-enable-filter t (interactive "P") (arg) "aurel.el") (aurel-filter-maintained t (interactive "P") (arg) "aurel.el") (aurel-filter-unmaintained t (interactive "P") (arg) "aurel.el") (aurel-filter-outdated t (interactive "P") (arg) "aurel.el") (aurel-filter-not-outdated t (interactive "P") (arg) "aurel.el") (aurel-filter-same-versions t (interactive "P") (arg) "aurel.el") (aurel-filter-different-versions t (interactive "P") (arg) "aurel.el") (aurel-filter-match-regexp t (interactive "P") (arg) "aurel.el") (aurel-filter-not-match-regexp t (interactive "P") (arg) "aurel.el") (aurel-list-download-package t (interactive nil) nil "aurel.el") (aurel-info-download-package t (interactive (list (bui-entry-value (aurel-read-entry-by-name (bui-current-entries)) 'git-url) (aurel-read-download-directory))) (url dir) "aurel.el") (aurel-info-vote-unvote t (interactive "P") (arg) "aurel.el") (aurel-info-subscribe-unsubscribe t (interactive "P") (arg) "aurel.el"))"#
        ]],
    )
}

fn aurel_custom_options_retain_exact_schema_defaults_and_groups() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_custom_options_retain_exact_schema_defaults_and_groups",
        r##"(mapcar
         (lambda (entry)
           (pcase-let
               ((`(,symbol ,group)
                 entry))
             (let ((type
                    (get
                     symbol
                     'custom-type))
                   (standard
                    (get
                     symbol
                     'standard-value)))
               (list
                symbol
                (symbol-value symbol)
                (and
                 (custom-variable-p symbol)
                 t)
                (if
                    (symbolp type)
                    type
                  (list
                   (car type)
                   (length type)
                   (mapcar
                    #'car-safe
                    (cdr type))))
                (list
                 (length standard)
                 (car-safe
                  (car standard)))
                (and
                 (member
                  (list
                   symbol
                   'custom-variable)
                  (get
                   group
                   'custom-group))
                 t)
                (file-name-nondirectory
                 (symbol-file
                  symbol
                  'defvar))))))
         '((aurel-aur-user-package-info-check
            aurel)
           (aurel-aur-user-name
            aurel)
           (aurel-pacman-program
            aurel)
           (aurel-installed-packages-check
            aurel)
           (aurel-download-directory
            aurel)
           (aurel-directory-prompt
            aurel)
           (aurel-list-download-function
            aurel-list)
           (aurel-list-multi-download-function
            aurel-list)
           (aurel-list-multi-download-no-confirm
            aurel-list)
           (aurel-info-download-function
            aurel-info)
           (aurel-info-voted-mark
            aurel-info)
           (aurel-info-display-voted-mark
            aurel-info)))"##,
        expect![[
            r#"OK ((aurel-aur-user-package-info-check nil t boolean (1 funcall) t "aurel.el") (aurel-aur-user-name "" t string (1 funcall) t "aurel.el") (aurel-pacman-program "/fixture/bin/pacman" t string (1 funcall) t "aurel.el") (aurel-installed-packages-check nil t boolean (1 funcall) t "aurel.el") (aurel-download-directory "/fixture/downloads/" t directory (1 funcall) t "aurel.el") (aurel-directory-prompt "Download to: " t string (1 funcall) t "aurel.el") (aurel-list-download-function aurel-download-dired t (radio 6 (function-item function-item function-item function-item function)) (1 funcall) t "aurel.el") (aurel-list-multi-download-function aurel-download t (radio 6 (function-item function-item function-item function-item function)) (1 funcall) t "aurel.el") (aurel-list-multi-download-no-confirm nil t boolean (1 funcall) t "aurel.el") (aurel-info-download-function aurel-download-dired t (radio 6 (function-item function-item function-item function-item function)) (1 funcall) t "aurel.el") (aurel-info-voted-mark "*" t string (1 funcall) t "aurel.el") (aurel-info-display-voted-mark t t boolean (1 funcall) t "aurel.el"))"#
        ]],
    )
}

fn aurel_keymaps_aliases_modes_and_face_specs_are_registered() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_keymaps_aliases_modes_and_face_specs_are_registered",
        r##"(list
         (mapcar
          (lambda (key)
            (list
             key
             (keymap-lookup
              aurel-filter-map
              key)))
          '("f"
            "v"
            "V"
            "m"
            "M"
            "o"
            "O"
            "r"
            "R"))
         (list
          (keymap-lookup
           aurel-list-mode-map
           "d")
          (eq
           (keymap-lookup
            aurel-list-mode-map
            "f")
           aurel-filter-map)
          (keymap-lookup
           aurel-info-mode-map
           "d")
          (keymap-lookup
           aurel-info-mode-map
           "v")
          (keymap-lookup
           aurel-info-mode-map
           "s"))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (symbol-function symbol)
             (get symbol 'byte-obsolete-info)))
          '(aurel-download-unpack-dired
            aurel-download-unpack-pkgbuild
            aurel-download-unpack-eshell))
         (mapcar
          (lambda (mode)
            (list
             mode
             (fboundp mode)
             (get mode
                  'derived-mode-parent)))
          '(aurel-list-mode
            aurel-info-mode))
         (mapcar
          (lambda (face)
            (list
             face
             (and
              (facep face)
              t)
             (get
              face
              'face-defface-spec)))
          '(aurel-info-name
            aurel-info-maintainer
            aurel-info-voted
            aurel-info-outdated)))"##,
        expect![[
            r#"OK ((("f" aurel-enable-filter) ("v" aurel-filter-same-versions) ("V" aurel-filter-different-versions) ("m" aurel-filter-unmaintained) ("M" aurel-filter-maintained) ("o" aurel-filter-outdated) ("O" aurel-filter-not-outdated) ("r" aurel-filter-not-match-regexp) ("R" aurel-filter-match-regexp)) (aurel-list-download-package t aurel-info-download-package aurel-info-vote-unvote aurel-info-subscribe-unsubscribe) ((aurel-download-unpack-dired aurel-download-dired (aurel-download-dired nil "0.10")) (aurel-download-unpack-pkgbuild aurel-download-pkgbuild (aurel-download-pkgbuild nil "0.10")) (aurel-download-unpack-eshell aurel-download-eshell (aurel-download-eshell nil "0.10"))) ((aurel-list-mode t bui-list-mode) (aurel-info-mode t bui-info-mode)) ((aurel-info-name t ((t :inherit font-lock-keyword-face))) (aurel-info-maintainer t ((t :inherit button))) (aurel-info-voted t ((default :weight bold) (((class color) (min-colors 88) (background light)) :foreground "ForestGreen") (((class color) (min-colors 88) (background dark)) :foreground "PaleGreen") (((class color) (min-colors 8)) :foreground "green") (t :underline t))) (aurel-info-outdated t ((t :inherit font-lock-warning-face)))))"#
        ]],
    )
}

fn aurel_found_messages_cover_real_search_cardinalities_and_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_found_messages_cover_real_search_cardinalities_and_arguments",
        r##"(let (events)
         (cl-letf
             (((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (let ((text
                        (apply
                         #'format
                         format-string
                         arguments)))
                   (push
                    (list
                     format-string
                     arguments
                     text)
                    events)
                   text))))
           (let ((one
                  '((1
                     (name . "resolved"))))
                 (many
                  '((1
                     (name . "one"))
                    (2
                     (name . "two"))
                    (3
                     (name . "three")))))
             (list
              (aurel-found-message
               nil
               'name
               "asked")
              (aurel-found-message
               nil
               'name
               "one"
               "two")
              (aurel-found-message
               one
               'name
               "alias")
              (aurel-found-message
               nil
               'string
               "fast"
               "editor")
              (aurel-found-message
               one
               'string
               "fast")
              (aurel-found-message
               many
               'string
               "fast"
               "editor")
              (aurel-found-message
               many
               'maintainer
               "Alice")
              (nreverse events)))))"##,
        expect![[
            r#"OK ("The package \"asked\" not found." "Packages not found." "The package \"resolved\"." "No packages matching \"fast\" \"editor\"." "A single package matching \"fast\"." "3 packages matching \"fast\" \"editor\"." "3 packages by maintainer Alice." (("The package \"%s\" not found." ("asked") "The package \"asked\" not found.") ("Packages not found." ("one") "Packages not found.") ("The package \"%s\"." ("resolved") "The package \"resolved\".") ("No packages matching %s." ("\"fast\" \"editor\"") "No packages matching \"fast\" \"editor\".") ("A single package matching %s." ("\"fast\"") "A single package matching \"fast\".") ("%d packages matching %s." (3 "\"fast\" \"editor\"") "3 packages matching \"fast\" \"editor\".") ("%d packages by maintainer %s." (3 "Alice") "3 packages by maintainer Alice.")))"#
        ]],
    )
}

fn aurel_source_reload_preserves_customized_state_and_runtime_histories() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_source_reload_preserves_customized_state_and_runtime_histories",
        r##"(let ((source
                (getenv
                 "NEOMACS_PACKAGE_SOURCE")))
         (setq aurel-aur-user-name
               "custom-user"
               aurel-pacman-program
               "/custom/pacman"
               aurel-download-directory
               "/custom/downloads/"
               aurel-package-info-history
               '("one")
               aurel-package-search-history
               '("two")
               aurel-maintainer-search-history
               '("three")
               aurel-filter-params
               '(name)
               aurel-filter-strings
               '("needle"))
         (load source nil t)
         (list
          aurel-aur-user-name
          aurel-pacman-program
          aurel-download-directory
          aurel-package-info-history
          aurel-package-search-history
          aurel-maintainer-search-history
          aurel-filter-params
          aurel-filter-strings
          (featurep 'aurel)))"##,
        expect![[
            r#"OK ("custom-user" "/custom/pacman" "/custom/downloads/" ("one") ("two") ("three") (name) ("needle") t)"#
        ]],
    )
}

fn aurel_generated_autoload_registers_all_five_user_entry_points() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_generated_autoload_registers_all_five_user_entry_points",
        r##"(let* ((file
                 (locate-library
                  "aurel-autoloads"))
                (history
                 (assoc file load-history))
                (prefix-files
                 (if
                     (hash-table-p
                      definition-prefixes)
                     (gethash
                      "aurel-"
                      definition-prefixes)
                   (cdr
                    (assoc
                     "aurel-"
                     definition-prefixes)))))
         (list
          (featurep
           'aurel-autoloads)
          (featurep
           'aurel)
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
            (cdr history)))
          (sort
           (delete-dups
            (copy-sequence
             prefix-files))
           #'string<)
          (mapcar
           (lambda (symbol)
             (list
              symbol
              (fboundp symbol)
              (autoloadp
               (symbol-function
                symbol))
              (commandp symbol)))
           '(aurel-package-info
             aurel-package-search
             aurel-package-search-by-name
             aurel-maintainer-search
             aurel-installed-packages))))"##,
        expect![[
            r#"OK (t nil ((defun aurel-package-info) (defun aurel-package-search) (defun aurel-package-search-by-name) (defun aurel-maintainer-search) (defun aurel-installed-packages) (provide aurel-autoloads)) ("aurel") ((aurel-package-info t t t) (aurel-package-search t t t) (aurel-package-search-by-name t t t) (aurel-maintainer-search t t t) (aurel-installed-packages t t t)))"#
        ]],
    )
}

pub(super) fn registry_aurel_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurel_descriptor_and_archive_sources_pin_exact_melpa_payload(),
        aurel_exact_bui_dependency_pin_is_active_with_both_features_loaded(),
        aurel_public_commands_have_exact_interactive_and_argument_contracts(),
        aurel_custom_options_retain_exact_schema_defaults_and_groups(),
        aurel_keymaps_aliases_modes_and_face_specs_are_registered(),
        aurel_found_messages_cover_real_search_cardinalities_and_arguments(),
        aurel_source_reload_preserves_customized_state_and_runtime_histories(),
    ]
}

pub(super) fn registry_aurel_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![aurel_generated_autoload_registers_all_five_user_entry_points()]
}
