use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_descriptor_pins_exact_release_origin_and_dependency_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_descriptor_pins_exact_release_origin_and_dependency_contract",
        r##"(let ((descriptor
                    (cadr
                     (assq 'asdf-vm
                           package-alist))))
               (list
                (package-desc-name descriptor)
                (package-version-join
                 (package-desc-version descriptor))
                (package-desc-summary descriptor)
                (package-desc-reqs descriptor)
                (package-desc-kind descriptor)
                (package-desc-archive descriptor)
                (package-desc-extras descriptor)))"##,
        expect![[
            r#"OK (asdf-vm "20250710.1053" "ASDF-VM porcelain." ((emacs (29 1))) nil nil ((:maintainers ("Zachary Elliott" . "contact@zell.io")) (:authors ("Zachary Elliott" . "contact@zell.io")) (:keywords "tools" "asdf-vm" "asdf") (:revdesc . "f6dbb4b6560c") (:commit . "f6dbb4b6560cd7e5bb05006e9fc416c5c323b567") (:url . "https://github.com/zellio/emacs-asdf-vm")))"#
        ]],
    )
}

fn asdf_vm_installed_payload_has_exact_inventory_sizes_and_source_hashes() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installed_payload_has_exact_inventory_sizes_and_source_hashes",
        r##"(let* ((descriptor
                     (cadr
                      (assq 'asdf-vm
                            package-alist)))
                    (directory
                     (package-desc-dir descriptor))
                    (files
                     (sort
                      (directory-files
                       directory t "^[^.].*")
                      #'string<)))
               (mapcar
                (lambda (file)
                  (list
                   (file-name-nondirectory file)
                   (file-attribute-size
                    (file-attributes file))
                   (and
                    (string-suffix-p
                     ".el"
                     file)
                    (with-temp-buffer
                      (insert-file-contents-literally
                       file)
                      (secure-hash
                       'sha256
                       (current-buffer))))))
                files))"##,
        expect![[
            r#"OK (("asdf-vm-autoloads.el" 12552 "82ffd39332d04ed32ec11349e4262bf736f39c82fbf27e6e72a80b9d019e150b") ("asdf-vm-config.el" 9670 "a963fdf15b88f4676802aa9dd0d917b438bbd95e2ae09e3b32b7f08caa9b6820") ("asdf-vm-config.elc" 11362 nil) ("asdf-vm-core.el" 12380 "517f627e1cc24b064bf1b1938acf70c4e1e6012d257ee44a6b99dc57fc42cc8a") ("asdf-vm-core.elc" 10330 nil) ("asdf-vm-error.el" 3008 "e3b227725e7dc656f6f1bfd8d8a7c13829f63eba46570530fc8fe2821cce1c7f") ("asdf-vm-error.elc" 1836 nil) ("asdf-vm-installer.el" 17212 "fe7b900de247383f528fb98ad4f2ed8954eaae0fec127ffa2dfb72efa2caf397") ("asdf-vm-installer.elc" 15927 nil) ("asdf-vm-mode.el" 9409 "901954533d770bc56b22e4262e832d87a920382685d59d3648b2efceaaa8df5e") ("asdf-vm-mode.elc" 10858 nil) ("asdf-vm-pkg.el" 418 "85461b041d32f4b1858922df580a63efab4cea0dbe7233e914deb01c86ffd438") ("asdf-vm-plugin-menu.el" 11429 "d9f6008dceb6345018f148e00fd7436efed093a2a48bef07cd938c39a9590c3b") ("asdf-vm-plugin-menu.elc" 12816 nil) ("asdf-vm-plugin.el" 7764 "39e12df7c5ca558f9c19e885ecc74b594e1e8e705a0dbdb125a61af863bb76f8") ("asdf-vm-plugin.elc" 6607 nil) ("asdf-vm-process.el" 10466 "5d0c21026ccea7615da9c457e77fee8565248165bac91a112b05e788eaa0f548") ("asdf-vm-process.elc" 8825 nil) ("asdf-vm-tool-versions.el" 7411 "49b70d367da0f05c2cf14c07dd994e5fe65c1e9b2a85525188e8a2aa25e1a486") ("asdf-vm-tool-versions.elc" 7651 nil) ("asdf-vm-ui.el" 4183 "e53a8348c6603c3001ad1b5b2fc7d9cb75dc6a93838999946632c5b77e69ace6") ("asdf-vm-ui.elc" 4691 nil) ("asdf-vm-util.el" 4516 "51f03162050e8e9cc527588f1f94f78a3a67e9d979aa4caa0c04cf308be4863d") ("asdf-vm-util.elc" 3278 nil) ("asdf-vm.el" 1749 "0ce7479509ce2a5bde65db403226313399365977209c1c5da39a09138c749a3f") ("asdf-vm.elc" 587 nil))"#
        ]],
    )
}

fn asdf_vm_complete_public_callable_surface_has_exact_command_and_arglist_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_complete_public_callable_surface_has_exact_command_and_arglist_contract",
        r##"(mapcar
               (lambda (function)
                 (list
                  function
                  (fboundp function)
                  (commandp function)
                  (help-function-arglist
                   function t)
                  (interactive-form
                   function)))
               '(asdf-vm-message
                 asdf-vm-config-edit
                 asdf-vm-current
                 asdf-vm-help
                 asdf-vm-install
                 asdf-vm-latest
                 asdf-vm-list
                 asdf-vm-list-all
                 asdf-vm-set
                 asdf-vm-uninstall
                 asdf-vm-where
                 asdf-vm-which
                 asdf-vm-info
                 asdf-vm-version
                 asdf-vm-reshim
                 asdf-vm-shim-versions
                 asdf-vm-installer-prefix-default
                 asdf-vm-installer-list-all
                 asdf-vm-installer-list-all-internal
                 asdf-vm-installer-download
                 asdf-vm-installer-install
                 asdf-vm-installer-list
                 asdf-vm-installer-list-internal
                 asdf-vm-installer-activate
                 asdf-vm-installer
                 asdf-vm-mode
                 asdf-vm-mode-enable
                 asdf-vm-mode-disable
                 asdf-vm-plugin-menu-mark-unmark
                 asdf-vm-plugin-menu-backup-unmark
                 asdf-vm-plugin-menu-mark-delete
                 asdf-vm-plugin-menu-mark-install
                 asdf-vm-plugin-browse-url
                 asdf-vm-plugin-menu-execute
                 asdf-vm-plugin-menu-mode
                 asdf-vm-plugin-menu
                 asdf-vm-plugin-add
                 asdf-vm-plugin-list
                 asdf-vm-plugin-list-all
                 asdf-vm-plugin-remove
                 asdf-vm-plugin-update
                 asdf-vm-plugin-update-all
                 asdf-vm-call
                 asdf-vm-tool-versions-edit))"##,
        expect![[
            r#"OK ((asdf-vm-message t nil (format-string &rest args) nil) (asdf-vm-config-edit t t (path) (interactive (list (if (or current-prefix-arg (not asdf-vm-config-file)) (read-file-name "ASDF-VM Config: " nil nil t) asdf-vm-config-file)))) (asdf-vm-current t t (&optional name interactive-call) (interactive (list (if current-prefix-arg nil (asdf-vm-plugin--installed-plugin-completing-read nil t)) (prefix-numeric-value current-prefix-arg)))) (asdf-vm-help t t (name &optional version) (interactive (let ((plugin (asdf-vm-plugin--installed-plugin-completing-read))) (list plugin (if current-prefix-arg (progn (completing-read "Plugin version: " (asdf-vm-list-all plugin)))))))) (asdf-vm-install t t (name &optional version interactive-call) (interactive (let ((package (asdf-vm-plugin--installed-plugin-completing-read nil t))) (list package (if current-prefix-arg nil (completing-read "Package version: " (asdf-vm-list-all package))) (prefix-numeric-value current-prefix-arg))))) (asdf-vm-latest t t (name &optional version-filter interactive-call) (interactive (list (asdf-vm-plugin--installed-plugin-completing-read nil t) (if current-prefix-arg (progn (read-string "Package version: "))) (prefix-numeric-value current-prefix-arg)))) (asdf-vm-list t t (name &optional version-filter interactive-call) (interactive (list (asdf-vm-plugin--installed-plugin-completing-read nil t) (if current-prefix-arg nil (read-string "Version filter: ")) (prefix-numeric-value current-prefix-arg)))) (asdf-vm-list-all t t (name &optional version-filter interactive-call) (interactive (list (asdf-vm-plugin--installed-plugin-completing-read nil t) (if current-prefix-arg nil (read-string "Version filter: ")) (prefix-numeric-value current-prefix-arg)))) (asdf-vm-set t t (name version &optional interactive-call) (interactive (let ((package (asdf-vm-plugin--installed-plugin-completing-read nil t))) (list package (asdf-vm--installed-package-version-completing-read package nil t) (prefix-numeric-value current-prefix-arg))))) (asdf-vm-uninstall t t (name version &optional interactive-call) (interactive (let ((package (asdf-vm-plugin--installed-plugin-completing-read nil t))) (list package (asdf-vm--installed-package-version-completing-read package) (prefix-numeric-value current-prefix-arg))))) (asdf-vm-where t t (name &optional version interactive-call) (interactive (let ((package (asdf-vm-plugin--installed-plugin-completing-read nil t))) (list package (if current-prefix-arg nil (asdf-vm--installed-package-version-completing-read package)) (prefix-numeric-value current-prefix-arg))))) (asdf-vm-which t t (command &optional interactive-call) (interactive "sCommand: \np")) (asdf-vm-info t t (&optional interactive-call) (interactive "p")) (asdf-vm-version t t (&optional interactive-call) (interactive "p")) (asdf-vm-reshim t t (name version &optional interactive-call) (interactive (let ((package (asdf-vm-plugin--installed-plugin-completing-read nil t))) (list package (completing-read "Package version: " (asdf-vm-list package)) (prefix-numeric-value current-prefix-arg))))) (asdf-vm-shim-versions t t (command &optional interactive-call) (interactive "sCommand: \np")) (asdf-vm-installer-prefix-default t nil nil nil) (asdf-vm-installer-list-all t t (&optional interactive-call) (interactive (list (prefix-numeric-value current-prefix-arg)))) (asdf-vm-installer-list-all-internal t nil nil nil) (asdf-vm-installer-download t t (version &optional interactive-call) (interactive (list (asdf-vm-installer--remote-version-completing-read) (prefix-numeric-value current-prefix-arg)))) (asdf-vm-installer-install t t (version &optional keep-downloads interactive-call) (interactive (list (asdf-vm-installer--remote-version-completing-read) current-prefix-arg (prefix-numeric-value current-prefix-arg)))) (asdf-vm-installer-list t t (&optional interactive-call) (interactive (list (prefix-numeric-value current-prefix-arg)))) (asdf-vm-installer-list-internal t nil nil nil) (asdf-vm-installer-activate t t (version &optional interactive-call) (interactive (list (asdf-vm-installer--local-version-completing-read) (prefix-numeric-value current-prefix-arg)))) (asdf-vm-installer t t (version &optional keep-downloads interactive-call) (interactive (list (asdf-vm-installer--remote-version-completing-read) current-prefix-arg (prefix-numeric-value current-prefix-arg)))) (asdf-vm-mode t t (&optional arg) (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle)))) (asdf-vm-mode-enable t nil nil nil) (asdf-vm-mode-disable t nil nil nil) (asdf-vm-plugin-menu-mark-unmark t t (&optional _) (interactive "p")) (asdf-vm-plugin-menu-backup-unmark t t (&optional _) (interactive "p")) (asdf-vm-plugin-menu-mark-delete t t (&optional _) (interactive "p")) (asdf-vm-plugin-menu-mark-install t t (&optional _) (interactive "p")) (asdf-vm-plugin-browse-url t t (url &optional secondary) (interactive (list (asdf-vm-plugin-menu--get-url) current-prefix-arg))) (asdf-vm-plugin-menu-execute t t (&optional _) (interactive "p")) (asdf-vm-plugin-menu-mode t nil nil nil) (asdf-vm-plugin-menu t t nil (interactive nil)) (asdf-vm-plugin-add t t (name git-url &optional interactive-call) (interactive (let* ((plugin (asdf-vm-plugin--plugin-completing-read nil t))) (list plugin (asdf-vm-plugin--git-url-read-string plugin) (prefix-numeric-value current-prefix-arg))))) (asdf-vm-plugin-list t t (&optional interactive-call) (interactive "p")) (asdf-vm-plugin-list-all t t (&optional interactive-call) (interactive "p")) (asdf-vm-plugin-remove t t (name &optional interactive-call) (interactive (list (asdf-vm-plugin--installed-plugin-completing-read nil t) (prefix-numeric-value current-prefix-arg)))) (asdf-vm-plugin-update t t (name &optional git-ref interactive-call) (interactive (list (asdf-vm-plugin--installed-plugin-completing-read nil t) (read-string "Plugin git ref: ") (prefix-numeric-value current-prefix-arg)))) (asdf-vm-plugin-update-all t t (&optional interactive-call) (interactive "p")) (asdf-vm-call t nil (&rest plist) nil) (asdf-vm-tool-versions-edit t t (&optional path) (interactive (list (if current-prefix-arg (read-file-name "ASDF-VM Tool Versions: ") (asdf-vm-tool-versions--locate-dominating-file))))))"#
        ]],
    )
}

fn asdf_vm_complete_public_variable_surface_has_exact_custom_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_complete_public_variable_surface_has_exact_custom_metadata",
        r##"(mapcar
               (lambda (symbol)
                 (list
                  symbol
                  (boundp symbol)
                  (get symbol
                       'custom-type)
                  (get symbol
                       'custom-group)
                  (get symbol
                       'safe-local-variable)
                  (get symbol
                       'variable-documentation)))
               '(asdf-vm-config-file
                 asdf-vm-tool-versions-filename
                 asdf-vm-dir
                 asdf-vm-data-dir-default
                 asdf-vm-data-dir
                 asdf-vm-concurrency
                 asdf-vm-help-buffer-name
                 asdf-vm-help-fill-column-width
                 asdf-vm-installer-prefix-default-function
                 asdf-vm-installer-prefix
                 asdf-vm-installer-exec-prefix
                 asdf-vm-installer-bin-dir
                 asdf-vm-installer-data-dir
                 asdf-vm-installer-src-dir
                 asdf-vm-installer-git-executable
                 asdf-vm-installer-git-arguments
                 asdf-vm-installer-md5sum-executable
                 asdf-vm-installer-md5sum-arguments
                 asdf-vm-installer-tar-executable
                 asdf-vm-installer-tar-arguments
                 asdf-vm-installer-github-url
                 asdf-vm-installer-git-repo-url
                 asdf-vm-installer-system
                 asdf-vm-installer-architecture
                 asdf-vm-mode-line-format
                 asdf-vm-core-command-map
                 asdf-vm-plugin-command-map
                 asdf-vm-installer-command-map
                 asdf-vm-mode-keymap-prefix
                 asdf-vm-mode-map
                 asdf-vm-mode
                 asdf-vm-path-injection-behaviour
                 asdf-vm-plugin-menu-buffer-name
                 asdf-vm-plugin-menu-list-padding
                 asdf-vm-plugin-menu-status-column-width
                 asdf-vm-plugin-menu-name-column-width
                 asdf-vm-plugin-menu-url-column-width
                 asdf-vm-plugin-menu-mode-map
                 asdf-vm-plugin-github-url
                 asdf-vm-plugin-repository-path
                 asdf-vm-process-executable
                 asdf-vm-process-executable-arguments
                 asdf-vm-process-buffer-name
                 asdf-vm-process-stderr-buffer-name))"##,
        expect![[
            r#"OK ((asdf-vm-config-file t string nil nil "Path to the .asdfrc configuration file.\n\nCan be set to any location. Must be an absolute path.") (asdf-vm-tool-versions-filename t string nil nil "The filename of the file storing the tool names and versions.\n\nMust be an absolute path.") (asdf-vm-dir t string nil nil "The location of ASDF-VM core scripts.\n\nCan be set to any location. Must be an absolute path.") (asdf-vm-data-dir-default t nil nil nil "Default value for `asdf-vm-data-dir'.") (asdf-vm-data-dir t string nil nil "The location where ASDF-VM will install plugins, shims and tool versions.\n\nCan be set to any location. Must be an absolute path.") (asdf-vm-concurrency t string nil nil "Number of cores to use when compiling the source code.") (asdf-vm-help-buffer-name t string nil nil "Display buffer for `asdf-vm-help' response.") (asdf-vm-help-fill-column-width t integer nil nil "Column width for `asdf-vm-help' display buffer formatting.") (asdf-vm-installer-prefix-default-function t function nil nil "Function to generate `asdf-vm-installer-prefix'.") (asdf-vm-installer-prefix t string nil nil "Installation PREFIX for `asdf-vm-installer'.") (asdf-vm-installer-exec-prefix t string nil nil "Installation EXEC-PREFIX for `asdf-vm-installer'.") (asdf-vm-installer-bin-dir t string nil nil "Installation BINDIR for `asdf-vm-installer'.") (asdf-vm-installer-data-dir t string nil nil "Read-only architecture-independent data.") (asdf-vm-installer-src-dir t string nil nil "Source file storage directory for `asdf-vm-installer'.") (asdf-vm-installer-git-executable t string nil nil "Path to git executable used in ASDF-VM installation.") (asdf-vm-installer-git-arguments t (repeat (string :tag "git argument")) nil nil "Optional arguments passed to git on every execution.") (asdf-vm-installer-md5sum-executable t string nil nil "Path to md5sum executable used in ASDF-VM installation.") (asdf-vm-installer-md5sum-arguments t (repeat (string :tag "md5sum argument")) nil nil "Optional arguments passed to md5sum on every execution.") (asdf-vm-installer-tar-executable t string nil nil "Path to tar executable used in ASDF-VM installation.") (asdf-vm-installer-tar-arguments t (repeat (string :tag "tar argument")) nil nil "Optional arguments passed to tar on every execution.") (asdf-vm-installer-github-url t string nil nil "Source url for ASDF-VM installation.") (asdf-vm-installer-git-repo-url t string nil nil "Git repository url for ASDF-VM installation.") (asdf-vm-installer-system t (choice (const "linux") (const "darwin")) nil nil "Operating system for ASDF-VM installation.") (asdf-vm-installer-architecture t (choice (const "amd64") (const "arm64") (const "386")) nil nil "Hardware architecture for ASDF-VM installation.") (asdf-vm-mode-line-format t sexpr nil nil "How `asdf-vm-mode' will indicate activity in the mode line.") (asdf-vm-core-command-map t nil nil nil "Command keymap for `asdf-vm-core'.") (asdf-vm-plugin-command-map t nil nil nil "Command keymap for `asdf-vm-plugin'.") (asdf-vm-installer-command-map t nil nil nil "Command keymap for `asdf-vm-installer'.") (asdf-vm-mode-keymap-prefix t string nil nil "Keymode map prefix for `asdf-vm-mode'.") (asdf-vm-mode-map t nil nil nil "Keymap for `asdf-vm-mode'.") (asdf-vm-mode t boolean nil nil "Non-nil if Asdf-Vm mode is enabled.\nSee the `asdf-vm-mode' command\nfor a description of this minor mode.\nSetting this variable directly does not take effect;\neither customize it (see the info node `Easy Customization')\nor call the function `asdf-vm-mode'.") (asdf-vm-path-injection-behaviour t (choice (const :tag "Prepend path directories" prepend) (const :tag "Append path directories" append) (const :tag "Do not update `exec-path'" nil)) nil nil "Control how ASDF-VM updates the variable `exec-path'.") (asdf-vm-plugin-menu-buffer-name t string nil nil "Display buffer name for `asdf-vm-plugin-menu'.") (asdf-vm-plugin-menu-list-padding t integer nil numberp "`tabulated-list-padding' for `asdf-vm-plugin-menu'.") (asdf-vm-plugin-menu-status-column-width t integer nil numberp "Column width for the status column of `asdf-vm-plugin-menu'.") (asdf-vm-plugin-menu-name-column-width t integer nil numberp "Column width for the name column of `asdf-vm-plugin-menu'.") (asdf-vm-plugin-menu-url-column-width t integer nil numberp "Column width for the repository url column of `asdf-vm-plugin-menu'.") (asdf-vm-plugin-menu-mode-map t nil nil nil "Local keymap for `asdf-vm-plugin-menu-mode' buffers.") (asdf-vm-plugin-github-url t string nil nil "Source url for ASDF-VM installation.") (asdf-vm-plugin-repository-path t string nil nil "Source url for ASDF-VM installation.") (asdf-vm-process-executable t string nil nil "Path to ASDF-VM command line tool.") (asdf-vm-process-executable-arguments t string nil nil "ASDF-VM command line tool execution arguments.\n\nThese values will be passed to every invocation of asdf before any\ncommand or command arguments.") (asdf-vm-process-buffer-name t string nil nil "Host buffer name for `asdf-vm-process' queue.") (asdf-vm-process-stderr-buffer-name t string nil nil "Host buffer name for ASDF-VM process stderr."))"#
        ]],
    )
}

fn asdf_vm_public_scalar_variable_defaults_form_one_exact_runtime_configuration() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_public_scalar_variable_defaults_form_one_exact_runtime_configuration",
        r##"(mapcar
               (lambda (symbol)
                 (let ((value
                        (symbol-value symbol)))
                   (list
                    symbol
                    (if
                        (memq
                         symbol
                         '(asdf-vm-installer-git-executable
                           asdf-vm-installer-md5sum-executable
                           asdf-vm-installer-tar-executable))
                        (list
                         (file-name-nondirectory
                          value)
                         (file-executable-p
                          value))
                      value))))
               '(asdf-vm-config-file
                 asdf-vm-tool-versions-filename
                 asdf-vm-dir
                 asdf-vm-data-dir-default
                 asdf-vm-data-dir
                 asdf-vm-concurrency
                 asdf-vm-help-buffer-name
                 asdf-vm-help-fill-column-width
                 asdf-vm-installer-prefix-default-function
                 asdf-vm-installer-prefix
                 asdf-vm-installer-exec-prefix
                 asdf-vm-installer-bin-dir
                 asdf-vm-installer-data-dir
                 asdf-vm-installer-src-dir
                 asdf-vm-installer-git-executable
                 asdf-vm-installer-git-arguments
                 asdf-vm-installer-md5sum-executable
                 asdf-vm-installer-md5sum-arguments
                 asdf-vm-installer-tar-executable
                 asdf-vm-installer-tar-arguments
                 asdf-vm-installer-github-url
                 asdf-vm-installer-git-repo-url
                 asdf-vm-installer-system
                 asdf-vm-installer-architecture
                 asdf-vm-mode-line-format
                 asdf-vm-mode-keymap-prefix
                 asdf-vm-mode
                 asdf-vm-path-injection-behaviour
                 asdf-vm-plugin-menu-buffer-name
                 asdf-vm-plugin-menu-list-padding
                 asdf-vm-plugin-menu-status-column-width
                 asdf-vm-plugin-menu-name-column-width
                 asdf-vm-plugin-menu-url-column-width
                 asdf-vm-plugin-github-url
                 asdf-vm-plugin-repository-path
                 asdf-vm-process-executable
                 asdf-vm-process-executable-arguments
                 asdf-vm-process-buffer-name
                 asdf-vm-process-stderr-buffer-name))"##,
        expect![[
            r#"OK ((asdf-vm-config-file "[ORACLE-HOME]/.asdfrc") (asdf-vm-tool-versions-filename ".tool-versions") (asdf-vm-dir "./") (asdf-vm-data-dir-default "./") (asdf-vm-data-dir "./") (asdf-vm-concurrency "auto") (asdf-vm-help-buffer-name "*asdf-vm-help*") (asdf-vm-help-fill-column-width 70) (asdf-vm-installer-prefix-default-function asdf-vm-installer-prefix-default) (asdf-vm-installer-prefix "[ORACLE-HOME]/.emacs.d/asdf") (asdf-vm-installer-exec-prefix "[ORACLE-HOME]/.emacs.d/asdf") (asdf-vm-installer-bin-dir "[ORACLE-HOME]/.emacs.d/asdf/bin") (asdf-vm-installer-data-dir "[ORACLE-HOME]/.emacs.d/asdf/share") (asdf-vm-installer-src-dir "[ORACLE-HOME]/.emacs.d/asdf/share/src") (asdf-vm-installer-git-executable ("git" t)) (asdf-vm-installer-git-arguments nil) (asdf-vm-installer-md5sum-executable ("md5sum" t)) (asdf-vm-installer-md5sum-arguments nil) (asdf-vm-installer-tar-executable ("tar" t)) (asdf-vm-installer-tar-arguments nil) (asdf-vm-installer-github-url "https://github.com/asdf-vm/asdf") (asdf-vm-installer-git-repo-url "https://github.com/asdf-vm/asdf.git") (asdf-vm-installer-system nil) (asdf-vm-installer-architecture nil) (asdf-vm-mode-line-format "(A)") (asdf-vm-mode-keymap-prefix "C-c a") (asdf-vm-mode nil) (asdf-vm-path-injection-behaviour prepend) (asdf-vm-plugin-menu-buffer-name "*ASDF-VM Plugins*") (asdf-vm-plugin-menu-list-padding 2) (asdf-vm-plugin-menu-status-column-width 10) (asdf-vm-plugin-menu-name-column-width 29) (asdf-vm-plugin-menu-url-column-width 0) (asdf-vm-plugin-github-url "https://github.com/asdf-vm/asdf-plugins") (asdf-vm-plugin-repository-path "[ORACLE-SANDBOX]/plugin-index") (asdf-vm-process-executable "asdf") (asdf-vm-process-executable-arguments nil) (asdf-vm-process-buffer-name "*asdf-vm*") (asdf-vm-process-stderr-buffer-name "*asdf-vm-stderr*"))"#
        ]],
    )
    .fresh_process()
}

fn asdf_vm_public_keymap_variable_values_bind_every_declared_command_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_public_keymap_variable_values_bind_every_declared_command_exactly",
        r##"(list
               (mapcar
                (lambda (spec)
                  (let ((map
                         (symbol-value
                          (car spec))))
                    (list
                     (car spec)
                     (keymapp map)
                     (mapcar
                      (lambda (key)
                        (list
                         key
                         (lookup-key
                          map
                          (kbd key))))
                      (cdr spec)))))
                '((asdf-vm-core-command-map
                   "e" "d" "c" "t" "C" "h" "i"
                   "a" "l" "L" "s" "u" "w" "W"
                   "n" "v" "r" "S")
                  (asdf-vm-plugin-command-map
                   "m" "a" "l" "L" "r" "u" "U")
                  (asdf-vm-installer-command-map
                   "i" "l" "L" "d" "I" "a")
                  (asdf-vm-plugin-menu-mode-map
                   "u" "DEL" "d" "i" "r" "w" "x")))
               (list
                (keymapp
                 asdf-vm-mode-map)
                (eq
                 (lookup-key
                  asdf-vm-mode-map
                  (kbd asdf-vm-mode-keymap-prefix))
                 asdf-vm-core-command-map)
                (eq
                 (lookup-key
                  asdf-vm-mode-map
                  (kbd
                   (concat
                    asdf-vm-mode-keymap-prefix
                    " P")))
                 asdf-vm-plugin-command-map)
                (eq
                 (lookup-key
                  asdf-vm-mode-map
                  (kbd
                   (concat
                    asdf-vm-mode-keymap-prefix
                    " I")))
                 asdf-vm-installer-command-map)))"##,
        expect![[
            r#"OK (((asdf-vm-core-command-map t (("e" asdf-vm-mode-enable) ("d" asdf-vm-mode-disable) ("c" asdf-vm-config-edit) ("t" asdf-vm-tool-versions-edit) ("C" asdf-vm-current) ("h" asdf-vm-help) ("i" asdf-vm-install) ("a" asdf-vm-latest) ("l" asdf-vm-list) ("L" asdf-vm-list-all) ("s" asdf-vm-set) ("u" asdf-vm-uninstall) ("w" asdf-vm-where) ("W" asdf-vm-which) ("n" asdf-vm-info) ("v" asdf-vm-version) ("r" asdf-vm-reshim) ("S" asdf-vm-shim-versions))) (asdf-vm-plugin-command-map t (("m" asdf-vm-plugin-menu) ("a" asdf-vm-plugin-add) ("l" asdf-vm-plugin-list) ("L" asdf-vm-plugin-list-all) ("r" asdf-vm-plugin-remove) ("u" asdf-vm-plugin-update) ("U" asdf-vm-plugin-update-all))) (asdf-vm-installer-command-map t (("i" asdf-vm-installer) ("l" asdf-vm-installer-list) ("L" asdf-vm-installer-list-all) ("d" asdf-vm-installer-download) ("I" asdf-vm-installer-install) ("a" asdf-vm-installer-activate))) (asdf-vm-plugin-menu-mode-map t (("u" asdf-vm-plugin-menu-mark-unmark) ("DEL" asdf-vm-plugin-menu-backup-unmark) ("d" asdf-vm-plugin-menu-mark-delete) ("i" asdf-vm-plugin-menu-mark-install) ("r" revert-buffer) ("w" asdf-vm-plugin-browse-url) ("x" asdf-vm-plugin-menu-execute)))) (t t t t))"#
        ]],
    )
}

fn asdf_vm_features_groups_faces_errors_and_default_keymaps_are_registered_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_features_groups_faces_errors_and_default_keymaps_are_registered_exactly",
        r##"(list
               (mapcar
                #'featurep
                '(asdf-vm
                  asdf-vm-error
                  asdf-vm-util
                  asdf-vm-process
                  asdf-vm-ui
                  asdf-vm-config
                  asdf-vm-installer
                  asdf-vm-plugin
                  asdf-vm-plugin-menu
                  asdf-vm-core
                  asdf-vm-tool-versions
                  asdf-vm-mode))
               (mapcar
                (lambda (group)
                  (list
                   group
                   (get group
                        'custom-group)))
                '(asdf-vm
                  asdf-vm-config
                  asdf-vm-installer
                  asdf-vm-plugin
                  asdf-vm-plugin-menu
                  asdf-vm-process
                  asdf-vm-tool-versions))
               (mapcar
                (lambda (face)
                  (list
                   face
                   (facep face)
                   (get face
                        'face-defface-spec)))
                '(asdf-vm-plugin-menu-status-available
                  asdf-vm-plugin-menu-status-installed))
               (mapcar
                (lambda (error)
                  (list
                   error
                   (get error
                        'error-conditions)
                   (get error
                        'error-message)))
                '(asdf-vm-error
                  asdf-vm-argument-missing
                  asdf-vm-plugin-error
                  asdf-vm-plugin-unreadable-repository-file
                  asdf-vm-plugin-menu-error
                  asdf-vm-incorrect-mode-error
                  asdf-vm-plugin-menu-missing-url-error
                  asdf-vm-process-error
                  asdf-vm-no-exectuable-error
                  asdf-vm-exec-error
                  asdf-vm-sentinel-error
                  asdf-vm-sentinel-nonsense-process-status
                  asdf-vm-sentinel-missing-process
                  asdf-vm-sentinel-unknown-status
                  asdf-vm-installer-error
                  asdf-vm-installer-unsupported-system
                  asdf-vm-installer-checksum-mismatch))
               (mapcar
                (lambda (key)
                  (list
                   key
                   (lookup-key
                    asdf-vm-mode-map
                    (kbd key))))
                '("C-c a e"
                  "C-c a C"
                  "C-c a P m"
                  "C-c a P U"
                  "C-c a I i"
                  "C-c a I a")))"##,
        expect![[
            r#"OK ((t t t t t t t t t t t t) ((asdf-vm ((asdf-vm-process custom-group) (asdf-vm-config custom-group) (asdf-vm-config-file custom-variable) (asdf-vm-installer custom-group) (asdf-vm-plugin custom-group) (asdf-vm-plugin-menu custom-group) (asdf-vm-help-buffer-name custom-variable) (asdf-vm-help-fill-column-width custom-variable) (asdf-vm-tool-versions custom-group) (asdf-vm-mode-line-format custom-variable) (asdf-vm-mode-keymap-prefix custom-variable) (asdf-vm-path-injection-behaviour custom-variable) (asdf-vm-mode custom-variable))) (asdf-vm-config ((asdf-vm-tool-versions-filename custom-variable) (asdf-vm-dir custom-variable) (asdf-vm-data-dir custom-variable) (asdf-vm-concurrency custom-variable))) (asdf-vm-installer ((asdf-vm-installer-prefix-default-function custom-variable) (asdf-vm-installer-prefix custom-variable) (asdf-vm-installer-exec-prefix custom-variable) (asdf-vm-installer-bin-dir custom-variable) (asdf-vm-installer-data-dir custom-variable) (asdf-vm-installer-src-dir custom-variable) (asdf-vm-installer-git-executable custom-variable) (asdf-vm-installer-git-arguments custom-variable) (asdf-vm-installer-md5sum-executable custom-variable) (asdf-vm-installer-md5sum-arguments custom-variable) (asdf-vm-installer-tar-executable custom-variable) (asdf-vm-installer-tar-arguments custom-variable) (asdf-vm-installer-github-url custom-variable) (asdf-vm-installer-git-repo-url custom-variable) (asdf-vm-installer-system custom-variable) (asdf-vm-installer-architecture custom-variable))) (asdf-vm-plugin ((asdf-vm-plugin-github-url custom-variable) (asdf-vm-plugin-repository-path custom-variable))) (asdf-vm-plugin-menu ((asdf-vm-plugin-menu-buffer-name custom-variable) (asdf-vm-plugin-menu-list-padding custom-variable) (asdf-vm-plugin-menu-status-column-width custom-variable) (asdf-vm-plugin-menu-name-column-width custom-variable) (asdf-vm-plugin-menu-url-column-width custom-variable) (asdf-vm-plugin-menu-status-available custom-face) (asdf-vm-plugin-menu-status-installed custom-face))) (asdf-vm-process ((asdf-vm-process-executable custom-variable) (asdf-vm-process-executable-arguments custom-variable) (asdf-vm-process-buffer-name custom-variable) (asdf-vm-process-stderr-buffer-name custom-variable))) (asdf-vm-tool-versions nil)) ((asdf-vm-plugin-menu-status-available [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t :inherit default))) (asdf-vm-plugin-menu-status-installed [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t :inherit font-lock-comment-face)))) ((asdf-vm-error (asdf-vm-error error) "[asdf-vm] Base error") (asdf-vm-argument-missing (asdf-vm-argument-missing asdf-vm-error error wrong-number-of-arguments) "[asdf-vm] Arguments missing from function call") (asdf-vm-plugin-error (asdf-vm-plugin-error asdf-vm-error error) "[asdf-vm] Base asdf-vm plugin error") (asdf-vm-plugin-unreadable-repository-file (asdf-vm-plugin-unreadable-repository-file asdf-vm-plugin-error asdf-vm-error error) "[asdf-vm] Cannot read repository file") (asdf-vm-plugin-menu-error (asdf-vm-plugin-menu-error asdf-vm-plugin-error asdf-vm-error error) "[asdf-vm] Base asdf-vm plugin menu error") (asdf-vm-incorrect-mode-error (asdf-vm-incorrect-mode-error asdf-vm-plugin-menu-error asdf-vm-plugin-error asdf-vm-error error) "[asdf-vm] The current buffer is not an ASDF-VM menu.") (asdf-vm-plugin-menu-missing-url-error (asdf-vm-plugin-menu-missing-url-error asdf-vm-plugin-menu-error asdf-vm-plugin-error asdf-vm-error error) "[asdf-vm] Plugin entry has to url") (asdf-vm-process-error (asdf-vm-process-error asdf-vm-error error) "[asdf-vm] Base process error") (asdf-vm-no-exectuable-error (asdf-vm-no-exectuable-error asdf-vm-process-error asdf-vm-error error) "[asdf-vm] No executable found") (asdf-vm-exec-error (asdf-vm-exec-error asdf-vm-process-error asdf-vm-error error) "[asdf-vm] Process exited with error status") (asdf-vm-sentinel-error (asdf-vm-sentinel-error asdf-vm-process-error asdf-vm-error error) "[asdf-vm] Base sentinel error") (asdf-vm-sentinel-nonsense-process-status (asdf-vm-sentinel-nonsense-process-status asdf-vm-sentinel-error asdf-vm-process-error asdf-vm-error error) "[asdf-vm] Nonsense process status in sentinel") (asdf-vm-sentinel-missing-process (asdf-vm-sentinel-missing-process asdf-vm-sentinel-error asdf-vm-process-error asdf-vm-error error) "[asdf-vm] Nonsense process status in sentinel") (asdf-vm-sentinel-unknown-status (asdf-vm-sentinel-unknown-status asdf-vm-sentinel-error asdf-vm-process-error asdf-vm-error error) "[asdf-vm] Unhanded process status in sentinel") (asdf-vm-installer-error (asdf-vm-installer-error asdf-vm-error error) "[asdf-vm] Base installer error") (asdf-vm-installer-unsupported-system (asdf-vm-installer-unsupported-system asdf-vm-installer-error asdf-vm-error error) "[asdf-vm] Detected system is not currently supported") (asdf-vm-installer-checksum-mismatch (asdf-vm-installer-checksum-mismatch asdf-vm-installer-error asdf-vm-error error) "[asdf-vm] Calculated and supplied checksum mismatch")) (("C-c a e" asdf-vm-mode-enable) ("C-c a C" asdf-vm-current) ("C-c a P m" asdf-vm-plugin-menu) ("C-c a P U" asdf-vm-plugin-update-all) ("C-c a I i" asdf-vm-installer) ("C-c a I a" asdf-vm-installer-activate)))"#
        ]],
    )
}

fn asdf_vm_generated_autoloads_expose_exact_user_commands_without_loading_runtime()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_generated_autoloads_expose_exact_user_commands_without_loading_runtime",
        r##"(list
               (featurep 'asdf-vm)
               (mapcar
                (lambda (function)
                  (list
                   function
                   (fboundp function)
                   (and
                    (fboundp function)
                    (autoloadp
                     (symbol-function
                      function)))
                   (commandp function)))
                '(asdf-vm-config-edit
                  asdf-vm-current
                  asdf-vm-help
                  asdf-vm-install
                  asdf-vm-latest
                  asdf-vm-list
                  asdf-vm-list-all
                  asdf-vm-set
                  asdf-vm-uninstall
                  asdf-vm-where
                  asdf-vm-which
                  asdf-vm-info
                  asdf-vm-version
                  asdf-vm-reshim
                  asdf-vm-shim-versions
                  asdf-vm-installer-list-all
                  asdf-vm-installer-download
                  asdf-vm-installer-install
                  asdf-vm-installer-list
                  asdf-vm-installer-activate
                  asdf-vm-installer
                  asdf-vm-mode
                  asdf-vm-mode-enable
                  asdf-vm-mode-disable
                  asdf-vm-plugin-menu
                  asdf-vm-plugin-add
                  asdf-vm-plugin-list
                  asdf-vm-plugin-list-all
                  asdf-vm-plugin-remove
                  asdf-vm-plugin-update
                  asdf-vm-plugin-update-all
                  asdf-vm-call
                  asdf-vm-tool-versions-edit))
               (boundp
                'asdf-vm-process-executable))"##,
        expect![
            "OK (nil ((asdf-vm-config-edit t t t) (asdf-vm-current t t t) (asdf-vm-help t t t) (asdf-vm-install t t t) (asdf-vm-latest t t t) (asdf-vm-list t t t) (asdf-vm-list-all t t t) (asdf-vm-set t t t) (asdf-vm-uninstall t t t) (asdf-vm-where t t t) (asdf-vm-which t t t) (asdf-vm-info t t t) (asdf-vm-version t t t) (asdf-vm-reshim t t t) (asdf-vm-shim-versions t t t) (asdf-vm-installer-list-all t t t) (asdf-vm-installer-download t t t) (asdf-vm-installer-install t t t) (asdf-vm-installer-list t t t) (asdf-vm-installer-activate t t t) (asdf-vm-installer t t t) (asdf-vm-mode t t t) (asdf-vm-mode-enable t t nil) (asdf-vm-mode-disable t t nil) (asdf-vm-plugin-menu t t t) (asdf-vm-plugin-add t t t) (asdf-vm-plugin-list t t t) (asdf-vm-plugin-list-all t t t) (asdf-vm-plugin-remove t t t) (asdf-vm-plugin-update t t t) (asdf-vm-plugin-update-all t t t) (asdf-vm-call t t nil) (asdf-vm-tool-versions-edit t t t)) nil)"
        ],
    )
}

pub(super) fn registry_asdf_vm_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_descriptor_pins_exact_release_origin_and_dependency_contract(),
        asdf_vm_installed_payload_has_exact_inventory_sizes_and_source_hashes(),
        asdf_vm_complete_public_callable_surface_has_exact_command_and_arglist_contract(),
        asdf_vm_complete_public_variable_surface_has_exact_custom_metadata(),
        asdf_vm_public_scalar_variable_defaults_form_one_exact_runtime_configuration(),
        asdf_vm_public_keymap_variable_values_bind_every_declared_command_exactly(),
        asdf_vm_features_groups_faces_errors_and_default_keymaps_are_registered_exactly(),
    ]
}

pub(super) fn registry_asdf_vm_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![asdf_vm_generated_autoloads_expose_exact_user_commands_without_loading_runtime()]
}
