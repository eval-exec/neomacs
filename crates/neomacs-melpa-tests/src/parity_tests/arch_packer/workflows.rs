use expect_test::expect;

use super::ParityBatchCase;

fn lists_installed_packages_refreshes_the_database_and_opens_detailed_kernel_info()
-> ParityBatchCase {
    ParityBatchCase::value(
        "lists_installed_packages_refreshes_the_database_and_opens_detailed_kernel_info",
        r##"(let* ((fixture
                         (neomacs-arch-packer-test-prepare
                          "arch-packer-list-workflow"))
                        (root (plist-get fixture :root))
                        (trace (plist-get fixture :trace))
                        (arch-packer-default-command "pacman")
                        (arch-packer-no-shell-history "")
                        result)
                   (unwind-protect
                       (progn
                         (arch-packer-list-packages)
                         (neomacs-arch-packer-test-wait-for
                          (lambda ()
                            (with-current-buffer
                                (get-buffer-create
                                 arch-packer-process-buffer)
                              (eq
                               major-mode
                               'arch-packer-package-menu-mode))))
                         (with-current-buffer
                             arch-packer-process-buffer
                           (goto-char (point-min))
                           (search-forward "linux")
                           (beginning-of-line)
                           (let* ((row-id
                                   (tabulated-list-get-id))
                                  (row
                                   (append
                                    (tabulated-list-get-entry)
                                    nil))
                                  (homepage
                                   (get-text-property
                                    0 'link (car row)))
                                  (menu
                                   (buffer-substring-no-properties
                                    (point-min) (point-max))))
                             (setq result
                                   (list
                                    :mode major-mode
                                    :rows
                                    (mapcar
                                     (lambda (entry)
                                       (substring-no-properties
                                        (car entry)))
                                     tabulated-list-entries)
                                    :menu menu
                                    :selected
                                    (list
                                     (substring-no-properties
                                      row-id)
                                     (mapcar
                                      #'substring-no-properties
                                      row)
                                     homepage)
                                    :info
                                    (progn
                                      (call-interactively
                                       #'arch-packer-pkg-info)
                                      (with-current-buffer
                                          "*pacman-package-info*"
                                        (let (faces)
                                          (dolist
                                              (needle
                                               '("Name"
                                                 "coreutils"))
                                            (goto-char
                                             (point-min))
                                            (search-forward
                                             needle)
                                            (push
                                             (cons
                                              needle
                                              (get-text-property
                                               (-
                                                (point)
                                                (length
                                                 needle))
                                               'font-lock-face))
                                             faces))
                                          (list
                                           major-mode
                                           buffer-read-only
                                           (buffer-substring-no-properties
                                            (point-min)
                                            (point-max))
                                           (nreverse faces)))))
                                    :trace
                                    (neomacs-arch-packer-test-file-string
                                     trace))))))
                     (neomacs-arch-packer-test-cleanup root))
                   result)"##,
        expect![[
            r##"OK (:mode arch-packer-package-menu-mode :rows ("local-helper" "linux" "ripgrep" "old-theme" "neovim") :menu "  local-helper       2.4-1                N/A                  Locally installed AUR helper\n  linux              6.9.1-1              6.9.1-1              The Linux kernel\n  ripgrep            14.1.0-1             14.1.0-1             Search recursively for a regex pattern\n  old-theme          1.0-2                1.0-2                Retired desktop theme\n  neovim             0.9.5-1              0.9.5-1              Installed modal editor awaiting a manual update\n" :selected ("linux" ("linux" "6.9.1-1" "6.9.1-1" "The Linux kernel") "https://archlinux.org/packages/core/x86_64/linux/") :info (special-mode t "Name            : linux\nVersion         : 6.9.1-1\nDepends On      : coreutils  kmod  mkinitcpio\nDescription     : The Linux kernel\nURL             : https://archlinux.org/packages/core/x86_64/linux/\nValidated By    : Signature\n" (("Name" :foreground "#6e8b3d") ("coreutils" :foreground "#b0e0e6"))) :trace "pacman <-Sy>\npacman <-Qu>\npacman <-Qe> <--info>\npacman <linux> <-Qe> <--info>\n")"##
        ]],
    )
}

fn searches_for_a_newer_editor_then_upgrades_the_installed_neovim_package() -> ParityBatchCase {
    ParityBatchCase::value(
        "searches_for_a_newer_editor_then_upgrades_the_installed_neovim_package",
        r##"(let* ((fixture
                         (neomacs-arch-packer-test-prepare
                          "arch-packer-search-install-workflow"))
                        (root (plist-get fixture :root))
                        (trace (plist-get fixture :trace))
                        (arch-packer-default-command "pacaur")
                        (arch-packer-no-shell-history "")
                        installed-before
                        result)
                   (unwind-protect
                       (progn
                         (arch-packer-list-packages)
                         (neomacs-arch-packer-test-wait-for
                          (lambda ()
                            (with-current-buffer
                                (get-buffer-create
                                 arch-packer-process-buffer)
                              (eq
                               major-mode
                               'arch-packer-package-menu-mode))))
                         (with-current-buffer
                             arch-packer-process-buffer
                           (set-window-buffer
                            (selected-window)
                            (current-buffer)
                            t)
                           (goto-char (point-min))
                           (search-forward "neovim")
                           (beginning-of-line)
                           (setq installed-before
                                 (mapcar
                                  #'substring-no-properties
                                  (append
                                   (tabulated-list-get-entry)
                                   nil)))
                           (execute-kbd-macro
                            (kbd "s editor RET")))
                         (neomacs-arch-packer-test-wait-for
                          (lambda ()
                            (with-current-buffer
                                (get-buffer-create
                                 arch-packer-process-buffer)
                              (eq
                               major-mode
                               'arch-packer-search-mode))))
                         (with-current-buffer
                             arch-packer-process-buffer
                           (set-window-buffer
                            (selected-window)
                            (current-buffer)
                            t)
                           (goto-char (point-min))
                           (search-forward "neovim")
                           (beginning-of-line)
                           (let ((search-menu
                                  (buffer-substring-no-properties
                                   (point-min) (point-max)))
                                 (selected
                                  (substring-no-properties
                                   (tabulated-list-get-id)))
                                 (repository
                                  (aref
                                   (tabulated-list-get-entry)
                                   2)))
                             (execute-kbd-macro
                              (kbd "i yes RET"))
                             (neomacs-arch-packer-test-wait-for
                             (lambda ()
                                (and
                                 (file-exists-p trace)
                                 (string-match-p
                                  "pacaur <-S> <--noconfirm> <neovim>"
                                  (neomacs-arch-packer-test-file-string
                                   trace))
                                 (with-current-buffer
                                     arch-packer-process-output-buffer
                                   (let ((text (buffer-string)))
                                     (and
                                      (string-match-p
                                       "resolving dependencies"
                                       text)
                                      (string-match-p
                                       "installing requested package"
                                       text)))))))
                             (setq result
                                   (list
                                    :installed-before
                                    installed-before
                                    :search-menu search-menu
                                    :selected
                                    (list
                                     selected
                                     (substring-no-properties
                                      repository))
                                    :output
                                    (with-current-buffer
                                        arch-packer-process-output-buffer
                                      (let ((text
                                             (buffer-string)))
                                        (list
                                         :resolved
                                         (and
                                          (string-match-p
                                           "resolving dependencies"
                                           text)
                                          t)
                                         :installed
                                         (and
                                          (string-match-p
                                           "installing requested package"
                                           text)
                                          t))))
                                    :trace
                                    (neomacs-arch-packer-test-trace-through
                                     trace
                                     "pacaur <-S> <--noconfirm> <neovim>"))))))
                     (neomacs-arch-packer-test-cleanup root))
                   result)"##,
        expect![[
            r#"OK (:installed-before ("neovim" "0.9.5-1" "0.9.5-1" "Installed modal editor awaiting a manual update") :search-menu "  neovim             0.10.0-2             extra           Fork of Vim focused on extensibility and usability\n  helix              24.3-1               extra           A post-modern modal text editor\n  emacs-git          30.0.50.r12345-1     aur             Development branch of the extensible editor\n" :selected ("neovim" "extra") :output (:resolved t :installed t) :trace "pacaur <-Sy>\npacman <-Qu>\npacman <-Qe> <--info>\npacaur <-Ss> <editor>\npacaur <-S> <--noconfirm> <neovim>\n")"#
        ]],
    )
    .fresh_process()
}

fn marks_an_obsolete_package_confirms_the_plan_and_executes_its_removal() -> ParityBatchCase {
    ParityBatchCase::value(
        "marks_an_obsolete_package_confirms_the_plan_and_executes_its_removal",
        r##"(let* ((fixture
                         (neomacs-arch-packer-test-prepare
                          "arch-packer-action-workflow"))
                        (root (plist-get fixture :root))
                        (trace (plist-get fixture :trace))
                        (arch-packer-default-command "pacman")
                        (arch-packer-no-shell-history "")
                        (post-command-hook nil)
                        result)
                   (unwind-protect
                       (progn
                         (arch-packer-list-packages)
                         (neomacs-arch-packer-test-wait-for
                          (lambda ()
                            (with-current-buffer
                                (get-buffer-create
                                 arch-packer-process-buffer)
                              (eq
                               major-mode
                               'arch-packer-package-menu-mode))))
                         (with-current-buffer
                             arch-packer-process-buffer
                           (set-window-buffer
                            (selected-window)
                            (current-buffer)
                            t)
                           (goto-char (point-min))
                           (search-forward "old-theme")
                           (beginning-of-line)
                           (call-interactively
                            #'arch-packer-menu-mark-delete)
                           (let ((marked-menu
                                  (buffer-substring-no-properties
                                   (point-min) (point-max))))
                             (execute-kbd-macro
                              (kbd "x yes RET"))
                             (neomacs-arch-packer-test-wait-for
                             (lambda ()
                                (and
                                 (file-exists-p trace)
                                 (string-match-p
                                  "pacman <-Rsn> <--noconfirm> <old-theme>"
                                  (neomacs-arch-packer-test-file-string
                                   trace))
                                 (with-current-buffer
                                     arch-packer-process-output-buffer
                                   (string-match-p
                                    "removing old-theme"
                                    (buffer-string))))))
                             (setq result
                                   (list
                                    :marked-menu marked-menu
                                    :progress-hook
                                    (and
                                     (memq
                                      'arch-packer-status-reporter
                                      post-command-hook)
                                     t)
                                    :output
                                    (with-current-buffer
                                        arch-packer-process-output-buffer
                                      (let ((text
                                             (buffer-string)))
                                        (list
                                         :removed
                                         (and
                                          (string-match-p
                                           "removing old-theme"
                                           text)
                                          t))))
                                    :trace
                                    (neomacs-arch-packer-test-trace-through
                                     trace
                                     "pacman <-Rsn> <--noconfirm> <old-theme>"))))))
                     (neomacs-arch-packer-test-cleanup root))
                   result)"##,
        expect![[
            r#"OK (:marked-menu "  local-helper       2.4-1                N/A                  Locally installed AUR helper\n  linux              6.9.1-1              6.9.1-1              The Linux kernel\n  ripgrep            14.1.0-1             14.1.0-1             Search recursively for a regex pattern\nD old-theme          1.0-2                1.0-2                Retired desktop theme\n  neovim             0.9.5-1              0.9.5-1              Installed modal editor awaiting a manual update\n" :progress-hook nil :output (:removed t) :trace "pacman <-Sy>\npacman <-Qu>\npacman <-Qe> <--info>\npacman <-Rsn> <--noconfirm> <old-theme>\n")"#
        ]],
    )
    .fresh_process()
}

fn refreshes_an_open_package_menu_after_the_repository_state_changes() -> ParityBatchCase {
    ParityBatchCase::value(
        "refreshes_an_open_package_menu_after_the_repository_state_changes",
        r##"(let* ((fixture
                         (neomacs-arch-packer-test-prepare
                          "arch-packer-refresh-workflow"))
                        (root (plist-get fixture :root))
                        (trace (plist-get fixture :trace))
                        (state (plist-get fixture :state))
                        (arch-packer-default-command "pacman")
                        (arch-packer-no-shell-history "")
                        result)
                   (unwind-protect
                       (progn
                         (arch-packer-list-packages)
                         (neomacs-arch-packer-test-wait-for
                          (lambda ()
                            (with-current-buffer
                                (get-buffer-create
                                 arch-packer-process-buffer)
                              (and
                               (eq
                                major-mode
                                'arch-packer-package-menu-mode)
                               (save-excursion
                                 (goto-char (point-min))
                                 (search-forward
                                  "The Linux kernel"
                                  nil t))))))
                         (with-temp-file state
                           (insert "refreshed\n"))
                         (with-current-buffer
                             arch-packer-process-buffer
                           (call-interactively
                            #'arch-packer-list-packages))
                         (neomacs-arch-packer-test-wait-for
                          (lambda ()
                            (with-current-buffer
                                arch-packer-process-buffer
                              (save-excursion
                                (goto-char (point-min))
                                (search-forward
                                 "Kernel after repository refresh"
                                 nil t)))))
                         (with-current-buffer
                             arch-packer-process-buffer
                           (goto-char (point-min))
                           (search-forward "linux")
                           (beginning-of-line)
                           (setq result
                                 (list
                                  :linux-row
                                  (mapcar
                                   #'substring-no-properties
                                   (append
                                    (tabulated-list-get-entry)
                                    nil))
                                  :menu
                                  (buffer-substring-no-properties
                                   (point-min) (point-max))
                                  :trace
                                  (neomacs-arch-packer-test-file-string
                                   trace)))))
                     (neomacs-arch-packer-test-cleanup root))
                   result)"##,
        expect![[
            r#"OK (:linux-row ("linux" "6.9.2-1" "6.9.2-1" "Kernel after repository refresh") :menu "  local-helper       2.4-1                N/A                  Locally installed AUR helper\n  linux              6.9.2-1              6.9.2-1              Kernel after repository refresh\n  ripgrep            14.1.0-1             14.1.0-1             Search recursively for a regex pattern\n  old-theme          1.0-2                1.0-2                Retired desktop theme\n  neovim             0.9.5-1              0.9.5-1              Installed modal editor awaiting a manual update\n" :trace "pacman <-Sy>\npacman <-Qu>\npacman <-Qe> <--info>\npacman <-Qu>\npacman <-Qe> <--info>\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        lists_installed_packages_refreshes_the_database_and_opens_detailed_kernel_info(),
        searches_for_a_newer_editor_then_upgrades_the_installed_neovim_package(),
        marks_an_obsolete_package_confirms_the_plan_and_executes_its_removal(),
        refreshes_an_open_package_menu_after_the_repository_state_changes(),
    ]
}
