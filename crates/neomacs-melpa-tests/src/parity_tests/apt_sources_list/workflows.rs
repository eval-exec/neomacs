use expect_test::expect;

use super::ParityBatchCase;

fn apt_sources_list_authors_fontifies_and_saves_a_multi_repository_configuration() -> ParityBatchCase
{
    ParityBatchCase::value(
        "apt_sources_list_authors_fontifies_and_saves_a_multi_repository_configuration",
        r##"(let* ((root
                  (apt-sources-list-test-root
                   "apt-sources-list-authoring"))
                 (path
                  (expand-file-name
                   "etc/apt/sources.list"
                   root))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apt-sources-list-test-cleanup root)
               (make-directory
                (file-name-directory path)
                t)
               (with-temp-file path
                 (insert
                  "# Managed Debian repositories\n"
                  "\n"
                  "deb https://deb.debian.org/debian stable main # primary\n"))
               (setq buffer
                     (find-file-noselect path))
               (with-current-buffer buffer
                 (goto-char
                  (point-min))
                 (search-forward
                  "deb https://")
                 (beginning-of-line)
                 (apt-sources-list-change-options
                  "arch=amd64 signed-by=/usr/share/keyrings/debian-archive-keyring.gpg")
                 (apt-sources-list-change-suite
                  "bookworm")
                 (apt-sources-list-change-components
                  "main contrib non-free-firmware")
                 (apt-sources-list-replicate)
                 (goto-char
                  (point-max))
                 (unless
                     (bolp)
                   (insert "\n"))
                 (apt-sources-list-insert
                  "https://security.debian.org/debian-security"
                  :name
                  "Debian security"
                  :options
                  "arch=amd64 signed-by=/usr/share/keyrings/debian-archive-keyring.gpg"
                  :suite
                  "bookworm-security"
                  :components
                  "main")
                 (insert "\n")
                 (font-lock-ensure)
                 (save-buffer)
                 (setq result
                       (list
                        :mode major-mode
                        :faces
                        (mapcar
                         (lambda (needle)
                           (goto-char
                            (point-min))
                           (search-forward needle)
                           (list
                            needle
                            (get-text-property
                             (-
                              (point)
                              (length needle))
                             'face)))
                         '("deb-src"
                           "arch=amd64"
                           "https://security.debian.org"
                           "bookworm-security"
                           "non-free-firmware"
                           "# primary"
                           "primary"))
                        :buffer
                        (buffer-substring-no-properties
                         (point-min)
                         (point-max))
                        :disk
                        (apt-sources-list-test-read-file
                         path)))))
           (apt-sources-list-test-cleanup root))
         result)"##,
        expect![[
            r##"OK (:mode apt-sources-list-mode :faces (("deb-src" apt-sources-list-type) ("arch=amd64" apt-sources-list-options) ("https://security.debian.org" apt-sources-list-uri) ("bookworm-security" apt-sources-list-suite) ("non-free-firmware" apt-sources-list-components) ("# primary" font-lock-comment-delimiter-face) ("primary" font-lock-comment-face)) :buffer "# Managed Debian repositories\n\ndeb [arch=amd64 signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://deb.debian.org/debian bookworm main contrib non-free-firmware # primary\ndeb-src [arch=amd64 signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://deb.debian.org/debian bookworm main contrib non-free-firmware # primary\n# Debian security\ndeb [arch=amd64 signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://security.debian.org/debian-security bookworm-security main\n" :disk "# Managed Debian repositories\n\ndeb [arch=amd64 signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://deb.debian.org/debian bookworm main contrib non-free-firmware # primary\ndeb-src [arch=amd64 signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://deb.debian.org/debian bookworm main contrib non-free-firmware # primary\n# Debian security\ndeb [arch=amd64 signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://security.debian.org/debian-security bookworm-security main\n")"##
        ]],
    )
}

fn apt_sources_list_keyboard_workflow_adds_copies_and_updates_a_security_repository()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apt_sources_list_keyboard_workflow_adds_copies_and_updates_a_security_repository",
        r##"(let* ((root
                  (apt-sources-list-test-root
                   "apt-sources-list-keyboard"))
                 (path
                  (expand-file-name
                   "etc/apt/sources.list.d/security.list"
                   root))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apt-sources-list-test-cleanup root)
               (make-directory
                (file-name-directory path)
                t)
               (with-temp-file path)
               (setq buffer
                     (find-file-noselect path))
               (set-window-buffer
                (selected-window)
                buffer
                t)
               (with-current-buffer buffer
                 (execute-kbd-macro
                  (kbd
                   "C-c C-i Debian SPC security RET security.debian.org/debian-security RET M-0 C-k bookworm-security RET M-0 C-k main SPC contrib RET"))
                 (goto-char
                  (point-min))
                 (search-forward
                  "\ndeb ")
                 (beginning-of-line)
                 (execute-kbd-macro
                  (kbd "C-c C-r"))
                 (execute-kbd-macro
                  (kbd "C-c C-o arch=amd64 RET"))
                 (execute-kbd-macro
                  (kbd
                   "C-c C-u M-0 C-k https://mirror.example/debian-security RET"))
                 (save-buffer)
                 (setq result
                       (list
                        :mode major-mode
                        :point-line
                        (line-number-at-pos)
                        :buffer
                        (buffer-substring-no-properties
                         (point-min)
                         (point-max))
                        :disk
                        (apt-sources-list-test-read-file
                         path)))))
           (apt-sources-list-test-cleanup root))
         result)"##,
        expect![[
            r##"OK (:mode apt-sources-list-mode :point-line 2 :buffer "# Debian security\ndeb [arch=amd64] https://mirror.example/debian-security bookworm-security main contrib\ndeb-src https://security.debian.org/debian-security bookworm-security main contrib\n" :disk "# Debian security\ndeb [arch=amd64] https://mirror.example/debian-security bookworm-security main contrib\ndeb-src https://security.debian.org/debian-security bookworm-security main contrib\n")"##
        ]],
    )
}

fn apt_sources_list_migrates_a_vendor_mirror_between_exact_and_component_suites() -> ParityBatchCase
{
    ParityBatchCase::value(
        "apt_sources_list_migrates_a_vendor_mirror_between_exact_and_component_suites",
        r##"(let* ((root
                  (apt-sources-list-test-root
                   "apt-sources-list-suite-migration"))
                 (path
                  (expand-file-name
                   "etc/apt/sources.list.d/vendor.list"
                   root))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apt-sources-list-test-cleanup root)
               (make-directory
                (file-name-directory path)
                t)
               (with-temp-file path
                 (insert
                  "deb [trusted=yes] file:/srv/vendor dists/bookworm/main/binary-amd64/ # exact local mirror\n"))
               (setq buffer
                     (find-file-noselect path))
               (with-current-buffer buffer
                 (goto-char
                  (point-min))
                 (apt-sources-list-change-uri
                  "file:/srv/vendor-v2")
                 (apt-sources-list-change-options
                  "arch=amd64 trusted=yes")
                 (apt-sources-list-change-suite
                  "bookworm"
                  "main contrib")
                 (let ((component-suite
                        (buffer-substring-no-properties
                         (point-min)
                         (point-max))))
                   (apt-sources-list-change-components
                    "main contrib non-free-firmware")
                   (let ((expanded-components
                          (buffer-substring-no-properties
                           (point-min)
                           (point-max))))
                     (apt-sources-list-change-suite
                      "dists/bookworm/main/binary-amd64/")
                     (let* ((exact-suite
                             (buffer-substring-no-properties
                              (point-min)
                              (point-max)))
                            (mismatch
                             (condition-case error
                                 (progn
                                   (apt-sources-list-change-components
                                    "main")
                                   'unexpected-success)
                               (error
                                (car error)))))
                       (save-buffer)
                       (setq result
                             (list
                              :component-suite
                              component-suite
                              :expanded-components
                              expanded-components
                              :exact-suite
                              exact-suite
                              :mismatch
                              mismatch
                              :unchanged-after-error
                              (equal
                               exact-suite
                               (buffer-substring-no-properties
                                (point-min)
                                (point-max)))
                              :disk
                              (apt-sources-list-test-read-file
                               path))))))))
           (apt-sources-list-test-cleanup root))
         result)"##,
        expect![[
            r##"OK (:component-suite "deb [arch=amd64 trusted=yes] file:/srv/vendor-v2 bookworm main contrib # exact local mirror\n" :expanded-components "deb [arch=amd64 trusted=yes] file:/srv/vendor-v2 bookworm main contrib non-free-firmware # exact local mirror\n" :exact-suite "deb [arch=amd64 trusted=yes] file:/srv/vendor-v2 dists/bookworm/main/binary-amd64/ # exact local mirror\n" :mismatch apt-sources-list-suite-component-mismatch :unchanged-after-error t :disk "deb [arch=amd64 trusted=yes] file:/srv/vendor-v2 dists/bookworm/main/binary-amd64/ # exact local mirror\n")"##
        ]],
    )
}

fn apt_sources_list_disables_reenables_and_navigates_repository_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "apt_sources_list_disables_reenables_and_navigates_repository_entries",
        r##"(let* ((root
                  (apt-sources-list-test-root
                   "apt-sources-list-enable-disable"))
                 (path
                  (expand-file-name
                   "etc/apt/sources.list"
                   root))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apt-sources-list-test-cleanup root)
               (make-directory
                (file-name-directory path)
                t)
               (with-temp-file path
                 (insert
                  "deb https://deb.debian.org/debian bookworm main\n"
                  "deb https://deb.debian.org/debian bookworm-updates main\n"
                  "# operator note\n"
                  "deb malformed repository\n"
                  "deb https://security.debian.org/debian-security bookworm-security main\n"))
               (setq buffer
                     (find-file-noselect path))
               (with-current-buffer buffer
                 (goto-char
                  (point-min))
                 (forward-line 1)
                 (let ((second-start
                        (line-beginning-position))
                       (second-end
                        (line-beginning-position 2)))
                   (comment-region
                    second-start
                    second-end))
                 (let ((disabled
                        (buffer-substring-no-properties
                         (point-min)
                         (point-max))))
                   (goto-char
                    (point-min))
                   (apt-sources-list-forward-source)
                   (let ((skipped-to
                          (string-trim-right
                           (thing-at-point
                            'line
                            t))))
                     (goto-char
                      (point-min))
                     (forward-line 1)
                     (let ((commented-start
                            (line-beginning-position))
                           (commented-end
                            (line-beginning-position 2)))
                       (uncomment-region
                        commented-start
                        commented-end))
                     (goto-char
                      (point-min))
                     (apt-sources-list-forward-source)
                     (let ((restored-next
                            (string-trim-right
                             (thing-at-point
                              'line
                              t))))
                       (apt-sources-list-change-components
                        "main contrib")
                       (save-buffer)
                       (setq result
                             (list
                              :disabled disabled
                              :skipped-to skipped-to
                              :restored-next restored-next
                              :final
                              (buffer-substring-no-properties
                               (point-min)
                               (point-max))
                              :disk
                              (apt-sources-list-test-read-file
                               path))))))))
           (apt-sources-list-test-cleanup root))
         result)"##,
        expect![[
            r##"OK (:disabled "deb https://deb.debian.org/debian bookworm main\n# deb https://deb.debian.org/debian bookworm-updates main\n# operator note\ndeb malformed repository\ndeb https://security.debian.org/debian-security bookworm-security main\n" :skipped-to "deb https://security.debian.org/debian-security bookworm-security main" :restored-next "deb https://deb.debian.org/debian bookworm-updates main" :final "deb https://deb.debian.org/debian bookworm main\ndeb https://deb.debian.org/debian bookworm-updates main contrib\n# operator note\ndeb malformed repository\ndeb https://security.debian.org/debian-security bookworm-security main\n" :disk "deb https://deb.debian.org/debian bookworm main\ndeb https://deb.debian.org/debian bookworm-updates main contrib\n# operator note\ndeb malformed repository\ndeb https://security.debian.org/debian-security bookworm-security main\n")"##
        ]],
    )
}

fn apt_sources_list_validation_rejects_malformed_edits_and_navigation_boundaries_without_damage()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apt_sources_list_validation_rejects_malformed_edits_and_navigation_boundaries_without_damage",
        r##"(let* ((root
                  (apt-sources-list-test-root
                   "apt-sources-list-validation"))
                 (path
                  (expand-file-name
                   "etc/apt/sources.list.d/audit.list"
                   root))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apt-sources-list-test-cleanup root)
               (make-directory
                (file-name-directory path)
                t)
               (with-temp-file path
                 (insert
                  "deb https://one.example/debian bookworm main\n"
                  "# retained comment\n"
                  "deb missing-uri-and-components\n"
                  "\n"
                  "deb-src [arch=arm64] https://two.example/debian bookworm main contrib\n"))
               (setq buffer
                     (find-file-noselect path))
               (with-current-buffer buffer
                 (let ((original
                        (buffer-substring-no-properties
                         (point-min)
                         (point-max)))
                       visits
                       malformed-status
                       malformed-error
                       boundary-error)
                   (goto-char
                    (point-min))
                   (push
                    (string-trim-right
                     (thing-at-point
                      'line
                      t))
                    visits)
                   (apt-sources-list-forward-source)
                   (push
                    (string-trim-right
                     (thing-at-point
                      'line
                      t))
                    visits)
                   (goto-char
                    (point-min))
                   (forward-line 2)
                   (setq malformed-status
                         (apt-sources-list-source-p)
                         malformed-error
                         (condition-case error
                             (progn
                               (apt-sources-list-change-uri
                                "https://fixed.example/debian")
                               'unexpected-success)
                           (error
                            (car error))))
                   (goto-char
                    (point-min))
                   (setq boundary-error
                         (condition-case error
                             (progn
                               (apt-sources-list-backward-source)
                               'unexpected-success)
                           (error
                            (list
                             (car error)
                             (cdr error)))))
                   (setq result
                         (list
                          :visits
                          (nreverse visits)
                          :malformed-source
                          malformed-status
                          :malformed-edit
                          malformed-error
                          :boundary
                          boundary-error
                          :buffer-unchanged
                          (equal
                           original
                           (buffer-substring-no-properties
                            (point-min)
                            (point-max)))
                          :disk-unchanged
                          (equal
                           original
                           (apt-sources-list-test-read-file
                            path)))))))
           (apt-sources-list-test-cleanup root))
         result)"##,
        expect![[
            r#"OK (:visits ("deb https://one.example/debian bookworm main" "deb-src [arch=arm64] https://two.example/debian bookworm main contrib") :malformed-source nil :malformed-edit apt-sources-list-not-found :boundary (error ("No further repositories found buffer")) :buffer-unchanged t :disk-unchanged t)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        apt_sources_list_authors_fontifies_and_saves_a_multi_repository_configuration(),
        apt_sources_list_keyboard_workflow_adds_copies_and_updates_a_security_repository(),
        apt_sources_list_migrates_a_vendor_mirror_between_exact_and_component_suites(),
        apt_sources_list_disables_reenables_and_navigates_repository_entries(),
        apt_sources_list_validation_rejects_malformed_edits_and_navigation_boundaries_without_damage(),
    ]
}
