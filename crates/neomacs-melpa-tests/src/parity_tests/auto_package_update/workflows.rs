use expect_test::expect;

use super::ParityBatchCase;

fn auto_package_update_real_local_archive_upgrades_package_and_removes_old_version()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_real_local_archive_upgrades_package_and_removes_old_version",
        r##"(let*
                             ((world
                               (auto-package-update-test-configure-local-world
                                "real-local-upgrade"))
                              (root
                               (plist-get world :root))
                              (archive
                               (plist-get world :archive))
                              (package-user-dir
                               (plist-get
                                world
                                :package-user-dir))
                              (auto-package-update-delete-old-versions
                               t)
                              (auto-package-update-hide-results
                               t)
                              (auto-package-update-buffer-name
                               " *apu-real-local-results*")
                              (auto-package-update-last-update-day-path
                               (plist-get world :day-file))
                              (old-directory nil)
                              before-events
                              after-events)
                           (auto-package-update-test-write-local-archive
                            archive
                            '((:name apu-alpha
                               :version (2 0)
                               :requirements
                               ((emacs (24 4)))
                               :body
                               "(defconst apu-alpha-generation \"new\")")))
                           (auto-package-update-test-install-local-version
                            root
                            'apu-alpha
                            '(1 0)
                            '((emacs (24 4)))
                            "(defconst apu-alpha-generation \"old\")")
                           (setq
                            old-directory
                            (package-desc-dir
                             (cadr
                              (assq
                               'apu-alpha
                               package-alist))))
                           (let
                               ((before
                                 (lambda ()
                                   (push
                                    (list
                                     :before
                                     (package-version-join
                                      (package-desc-version
                                       (cadr
                                        (assq
                                         'apu-alpha
                                         package-alist)))))
                                    before-events)))
                                (after
                                 (lambda ()
                                   (push
                                    (list
                                     :after
                                     (package-version-join
                                      (package-desc-version
                                       (cadr
                                        (assq
                                         'apu-alpha
                                         package-alist)))))
                                    after-events))))
                             (let
                                 ((auto-package-update-before-hook
                                   (list before))
                                  (auto-package-update-after-hook
                                   (list after)))
                               (unwind-protect
                                   (cl-letf
                                       (((symbol-function
                                          'apu--today-day)
                                         (lambda () 4242)))
                                     (auto-package-update-now)
                                     (let*
                                         ((installed
                                           (cadr
                                            (assq
                                             'apu-alpha
                                             package-alist)))
                                          (installed-directory
                                           (package-desc-dir
                                            installed))
                                          (installed-source
                                           (expand-file-name
                                            "apu-alpha.el"
                                            installed-directory)))
                                       (with-current-buffer
                                           auto-package-update-buffer-name
                                         (list
                                          (package-version-join
                                           (package-desc-version
                                            installed))
                                          (file-exists-p
                                           old-directory)
                                          (file-readable-p
                                           installed-source)
                                          (with-temp-buffer
                                            (insert-file-contents
                                             installed-source)
                                            (and
                                             (search-forward
                                              "apu-alpha-generation \"new\""
                                              nil
                                              t)
                                             :new-source))
                                          (auto-package-update-test-read
                                           auto-package-update-last-update-day-path)
                                          (buffer-string)
                                          buffer-read-only
                                          auto-package-update-minor-mode
                                          (nreverse before-events)
                                          (nreverse after-events)
                                          (mapcar
                                           (lambda (description)
                                             (package-version-join
                                              (package-desc-version
                                               description)))
                                           (cdr
                                            (assq
                                             'apu-alpha
                                             package-alist)))
                                          (file-readable-p
                                           (expand-file-name
                                            "archives/fixture/archive-contents"
                                            package-user-dir))))))
                                 (auto-package-update-test-kill-buffers
                                  auto-package-update-buffer-name)))))"##,
        expect![[
            r#"OK ("2.0" nil t :new-source "4242" "[PACKAGES UPDATED]:\napu-alpha up to date." t t ((:before "1.0")) ((:after "2.0")) ("2.0" "1.0") t)"#
        ]],
    )
    .fresh_process()
}

fn auto_package_update_real_local_archive_installs_transitive_dependency_and_runs_new_code()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_real_local_archive_installs_transitive_dependency_and_runs_new_code",
        r##"(let*
                             ((world
                               (auto-package-update-test-configure-local-world
                                "real-local-dependency"))
                              (root
                               (plist-get world :root))
                              (archive
                               (plist-get world :archive))
                              (auto-package-update-delete-old-versions
                               t)
                              (auto-package-update-hide-results
                               t)
                              (auto-package-update-buffer-name
                               " *apu-real-dependency-results*")
                              (auto-package-update-last-update-day-path
                               (plist-get world :day-file))
                              old-app-directory)
                           (auto-package-update-test-write-local-archive
                            archive
                            '((:name apu-dep
                               :version (1 0)
                               :requirements
                               ((emacs (24 4)))
                               :body
                               "(defun apu-dep-value () 41)")
                              (:name apu-app
                               :version (2 0)
                               :requirements
                               ((emacs (24 4))
                                (apu-dep (1 0)))
                               :body
                               "(require 'apu-dep)\n(defun apu-app-value () (list :app (apu-dep-value)))")))
                           (auto-package-update-test-install-local-version
                            root
                            'apu-app
                            '(1 0)
                            '((emacs (24 4)))
                            "(defun apu-app-value () '(:app old))")
                           (setq
                            old-app-directory
                            (package-desc-dir
                             (auto-package-update-test-installed-description
                              'apu-app)))
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'apu--today-day)
                                     (lambda () 5151)))
                                 (auto-package-update-now)
                                 (load
                                  (auto-package-update-test-installed-source
                                   'apu-app)
                                  nil
                                  t
                                  t)
                                 (with-current-buffer
                                     auto-package-update-buffer-name
                                   (list
                                    (auto-package-update-test-installed-version
                                     'apu-app)
                                    (auto-package-update-test-installed-version
                                     'apu-dep)
                                    (apu-app-value)
                                    (and
                                     (auto-package-update-test-installed-source-contains-p
                                      'apu-app
                                      "(require 'apu-dep)")
                                     :app-requires-dependency)
                                    (and
                                     (auto-package-update-test-installed-source-contains-p
                                      'apu-dep
                                      "apu-dep-value () 41")
                                     :dependency-source)
                                    (file-exists-p
                                     old-app-directory)
                                    (auto-package-update-test-read
                                     auto-package-update-last-update-day-path)
                                    (buffer-string)
                                    (mapcar
                                     #'car
                                     package-alist)
                                    (file-readable-p
                                     (expand-file-name
                                      "apu-dep-1.0.el"
                                      archive))
                                    (file-readable-p
                                     (expand-file-name
                                      "apu-app-2.0.el"
                                      archive)))))
                             (auto-package-update-test-kill-buffers
                              auto-package-update-buffer-name)))"##,
        expect![[
            r#"OK ("2.0" "1.0" (:app 41) :app-requires-dependency :dependency-source nil "5151" "[PACKAGES UPDATED]:\napu-app up to date." (apu-dep apu-app dash) t t)"#
        ]],
    )
    .fresh_process()
}

fn auto_package_update_real_local_archive_respects_exclusion_and_leaves_package_untouched()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_real_local_archive_respects_exclusion_and_leaves_package_untouched",
        r##"(let*
                             ((world
                               (auto-package-update-test-configure-local-world
                                "real-local-exclusion"))
                              (root
                               (plist-get world :root))
                              (archive
                               (plist-get world :archive))
                              (auto-package-update-excluded-packages
                               '(apu-beta))
                              (auto-package-update-delete-old-versions
                               t)
                              (auto-package-update-hide-results
                               t)
                              (auto-package-update-buffer-name
                               " *apu-real-exclusion-results*")
                              (auto-package-update-last-update-day-path
                               (plist-get world :day-file))
                              old-alpha-directory
                              old-beta-directory)
                           (auto-package-update-test-write-local-archive
                            archive
                            '((:name apu-alpha
                               :version (2 0)
                               :requirements
                               ((emacs (24 4)))
                               :body
                               "(defconst apu-alpha-generation \"new\")")
                              (:name apu-beta
                               :version (2 0)
                               :requirements
                               ((emacs (24 4)))
                               :body
                               "(defconst apu-beta-generation \"new\")")))
                           (auto-package-update-test-install-local-version
                            root
                            'apu-alpha
                            '(1 0)
                            '((emacs (24 4)))
                            "(defconst apu-alpha-generation \"old\")")
                           (auto-package-update-test-install-local-version
                            root
                            'apu-beta
                            '(1 0)
                            '((emacs (24 4)))
                            "(defconst apu-beta-generation \"old\")")
                           (setq
                            old-alpha-directory
                            (package-desc-dir
                             (auto-package-update-test-installed-description
                              'apu-alpha))
                            old-beta-directory
                            (package-desc-dir
                             (auto-package-update-test-installed-description
                              'apu-beta)))
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'apu--today-day)
                                     (lambda () 6262)))
                                 (auto-package-update-now)
                                 (with-current-buffer
                                     auto-package-update-buffer-name
                                   (list
                                    (auto-package-update-test-installed-version
                                     'apu-alpha)
                                    (auto-package-update-test-installed-version
                                     'apu-beta)
                                    (and
                                     (auto-package-update-test-installed-source-contains-p
                                      'apu-alpha
                                      "apu-alpha-generation \"new\"")
                                     :alpha-new)
                                    (and
                                     (auto-package-update-test-installed-source-contains-p
                                      'apu-beta
                                      "apu-beta-generation \"old\"")
                                     :beta-old)
                                    (file-exists-p
                                     old-alpha-directory)
                                    (file-exists-p
                                     old-beta-directory)
                                    (auto-package-update-test-read
                                     auto-package-update-last-update-day-path)
                                    (buffer-string)
                                    (mapcar
                                     (lambda (description)
                                       (package-version-join
                                        (package-desc-version
                                         description)))
                                     (cdr
                                      (assq
                                       'apu-beta
                                       package-alist))))))
                             (auto-package-update-test-kill-buffers
                              auto-package-update-buffer-name)))"##,
        expect![[
            r#"OK ("2.0" "1.0" :alpha-new :beta-old nil t "6262" "[PACKAGES UPDATED]:\napu-alpha up to date." ("1.0"))"#
        ]],
    )
    .fresh_process()
}

fn auto_package_update_real_prompt_denial_preserves_old_package_then_acceptance_updates_it()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_real_prompt_denial_preserves_old_package_then_acceptance_updates_it",
        r##"(let*
                             ((world
                               (auto-package-update-test-configure-local-world
                                "real-local-prompt"))
                              (root
                               (plist-get world :root))
                              (archive
                               (plist-get world :archive))
                              (package-user-dir
                               (plist-get
                                world
                                :package-user-dir))
                              (auto-package-update-prompt-before-update
                               t)
                              (auto-package-update-show-preview
                               t)
                              (auto-package-update-delete-old-versions
                               t)
                              (auto-package-update-hide-results
                               t)
                              (auto-package-preview-buffer-name
                               " *apu-real-prompt-preview*")
                              (auto-package-update-buffer-name
                               " *apu-real-prompt-results*")
                              (auto-package-update-last-update-day-path
                               (plist-get world :day-file))
                              (answer nil)
                              prompts
                              denied-state
                              accepted-state
                              old-directory)
                           (auto-package-update-test-write-local-archive
                            archive
                            '((:name apu-alpha
                               :version (2 0)
                               :requirements
                               ((emacs (24 4)))
                               :body
                               "(defconst apu-alpha-generation \"accepted\")")))
                           (auto-package-update-test-install-local-version
                            root
                            'apu-alpha
                            '(1 0)
                            '((emacs (24 4)))
                            "(defconst apu-alpha-generation \"denied\")")
                           (setq
                            old-directory
                            (package-desc-dir
                             (auto-package-update-test-installed-description
                              'apu-alpha)))
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'apu--today-day)
                                     (lambda () 7373))
                                    ((symbol-function
                                      'y-or-n-p)
                                     (lambda (question)
                                       (push question prompts)
                                       answer)))
                                 (auto-package-update-maybe)
                                 (setq
                                  denied-state
                                  (list
                                   (auto-package-update-test-installed-version
                                    'apu-alpha)
                                   (and
                                    (auto-package-update-test-installed-source-contains-p
                                     'apu-alpha
                                     "apu-alpha-generation \"denied\"")
                                    :old-source)
                                   (file-exists-p
                                    auto-package-update-last-update-day-path)
                                   (get-buffer
                                    auto-package-preview-buffer-name)
                                   (get-buffer
                                    auto-package-update-buffer-name)
                                   (file-readable-p
                                    (expand-file-name
                                     "archives/fixture/archive-contents"
                                     package-user-dir))))
                                 (setq answer t)
                                 (auto-package-update-maybe)
                                 (setq
                                  accepted-state
                                  (with-current-buffer
                                      auto-package-update-buffer-name
                                    (list
                                     (auto-package-update-test-installed-version
                                      'apu-alpha)
                                     (and
                                      (auto-package-update-test-installed-source-contains-p
                                       'apu-alpha
                                       "apu-alpha-generation \"accepted\"")
                                      :new-source)
                                     (file-exists-p
                                      old-directory)
                                     (auto-package-update-test-read
                                      auto-package-update-last-update-day-path)
                                     (buffer-string)
                                     buffer-read-only
                                     (get-buffer
                                      auto-package-preview-buffer-name))))
                                 (list
                                  denied-state
                                  accepted-state
                                  (nreverse prompts)))
                             (auto-package-update-test-kill-buffers
                              auto-package-preview-buffer-name
                              auto-package-update-buffer-name)))"##,
        expect![[
            r#"OK (("1.0" :old-source nil nil nil t) ("2.0" :new-source nil "7373" "[PACKAGES UPDATED]:\napu-alpha up to date." t nil) ("Auto-update packages now?" "Auto-update packages now?"))"#
        ]],
    )
    .fresh_process()
}

fn auto_package_update_real_missing_archive_payload_reports_failure_and_preserves_installed_package()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_real_missing_archive_payload_reports_failure_and_preserves_installed_package",
        r##"(let*
                             ((world
                               (auto-package-update-test-configure-local-world
                                "real-local-failure"))
                              (root
                               (plist-get world :root))
                              (archive
                               (plist-get world :archive))
                              (package-user-dir
                               (plist-get
                                world
                                :package-user-dir))
                              (auto-package-update-delete-old-versions
                               nil)
                              (auto-package-update-hide-results
                               t)
                              (auto-package-update-buffer-name
                               " *apu-real-failure-results*")
                              (auto-package-update-last-update-day-path
                               (plist-get world :day-file))
                              old-directory
                              run-outcome
                              events)
                           (auto-package-update-test-write-local-archive
                            archive
                            '((:name apu-alpha
                               :version (2 0)
                               :requirements
                               ((emacs (24 4)))
                               :body
                               "(defconst apu-alpha-generation \"missing\")")))
                           (delete-file
                            (expand-file-name
                             "apu-alpha-2.0.el"
                             archive))
                           (auto-package-update-test-install-local-version
                            root
                            'apu-alpha
                            '(1 0)
                            '((emacs (24 4)))
                            "(defconst apu-alpha-generation \"survives\")")
                           (setq
                            old-directory
                            (package-desc-dir
                             (auto-package-update-test-installed-description
                              'apu-alpha)))
                           (let
                               ((before
                                 (lambda ()
                                   (push :before events)))
                                (after
                                 (lambda ()
                                   (push :after events))))
                             (let
                                 ((auto-package-update-before-hook
                                   (list before))
                                  (auto-package-update-after-hook
                                   (list after)))
                               (unwind-protect
                                   (cl-letf
                                       (((symbol-function
                                          'apu--today-day)
                                         (lambda () 8484)))
                                     (setq
                                      run-outcome
                                      (auto-package-update-test-error
                                       #'auto-package-update-now))
                                     (list
                                      run-outcome
                                      (auto-package-update-test-installed-version
                                       'apu-alpha)
                                      (and
                                       (auto-package-update-test-installed-source-contains-p
                                        'apu-alpha
                                        "apu-alpha-generation \"survives\"")
                                       :old-source)
                                      (file-exists-p
                                       old-directory)
                                      (file-exists-p
                                       (expand-file-name
                                        "apu-alpha-2.0"
                                        package-user-dir))
                                      (and
                                       (file-exists-p
                                        auto-package-update-last-update-day-path)
                                       (auto-package-update-test-read
                                        auto-package-update-last-update-day-path))
                                      (let ((buffer
                                             (get-buffer
                                              auto-package-update-buffer-name)))
                                        (and
                                         buffer
                                         (with-current-buffer buffer
                                           (buffer-string))))
                                      (nreverse events)
                                      (file-readable-p
                                       (expand-file-name
                                        "archives/fixture/archive-contents"
                                        package-user-dir))))
                                 (auto-package-update-test-kill-buffers
                                  auto-package-update-buffer-name)))))"##,
        expect![[
            r#"OK ((:value nil) "1.0" :old-source t nil "8484" "[PACKAGES UPDATED]:\nError installing apu-alpha" (:before :after) t)"#
        ]],
    )
    .fresh_process()
}

fn auto_package_update_real_schedule_skips_before_interval_then_updates_at_due_boundary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_real_schedule_skips_before_interval_then_updates_at_due_boundary",
        r##"(let*
                             ((world
                               (auto-package-update-test-configure-local-world
                                "real-local-schedule"))
                              (root
                               (plist-get world :root))
                              (archive
                               (plist-get world :archive))
                              (package-user-dir
                               (plist-get
                                world
                                :package-user-dir))
                              (auto-package-update-interval 7)
                              (auto-package-update-prompt-before-update
                               nil)
                              (auto-package-update-delete-old-versions
                               t)
                              (auto-package-update-hide-results
                               t)
                              (auto-package-update-buffer-name
                               " *apu-real-schedule-results*")
                              (auto-package-update-last-update-day-path
                               (plist-get world :day-file))
                              (today 106)
                              before-due
                              at-due)
                           (auto-package-update-test-write-local-archive
                            archive
                            '((:name apu-alpha
                               :version (2 0)
                               :requirements
                               ((emacs (24 4)))
                               :body
                               "(defconst apu-alpha-generation \"due\")")))
                           (auto-package-update-test-install-local-version
                            root
                            'apu-alpha
                            '(1 0)
                            '((emacs (24 4)))
                            "(defconst apu-alpha-generation \"waiting\")")
                           (auto-package-update-test-write
                            auto-package-update-last-update-day-path
                            "100")
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'apu--today-day)
                                     (lambda () today)))
                                 (auto-package-update-maybe)
                                 (setq
                                  before-due
                                  (list
                                   (auto-package-update-test-installed-version
                                    'apu-alpha)
                                   (and
                                    (auto-package-update-test-installed-source-contains-p
                                     'apu-alpha
                                     "apu-alpha-generation \"waiting\"")
                                    :waiting)
                                   (file-readable-p
                                    (expand-file-name
                                     "archives/fixture/archive-contents"
                                     package-user-dir))
                                   (auto-package-update-test-read
                                    auto-package-update-last-update-day-path)
                                   (get-buffer
                                    auto-package-update-buffer-name)))
                                 (setq today 107)
                                 (auto-package-update-maybe)
                                 (setq
                                  at-due
                                  (with-current-buffer
                                      auto-package-update-buffer-name
                                    (list
                                     (auto-package-update-test-installed-version
                                      'apu-alpha)
                                     (and
                                      (auto-package-update-test-installed-source-contains-p
                                       'apu-alpha
                                       "apu-alpha-generation \"due\"")
                                      :updated)
                                     (file-readable-p
                                      (expand-file-name
                                       "archives/fixture/archive-contents"
                                       package-user-dir))
                                     (auto-package-update-test-read
                                      auto-package-update-last-update-day-path)
                                     (buffer-string))))
                                 (list before-due at-due))
                             (auto-package-update-test-kill-buffers
                              auto-package-update-buffer-name)))"##,
        expect![[
            r#"OK (("1.0" :waiting nil "100" nil) ("2.0" :updated t "107" "[PACKAGES UPDATED]:\napu-alpha up to date."))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_package_update_real_local_archive_upgrades_package_and_removes_old_version(),
        auto_package_update_real_local_archive_installs_transitive_dependency_and_runs_new_code(),
        auto_package_update_real_local_archive_respects_exclusion_and_leaves_package_untouched(),
        auto_package_update_real_prompt_denial_preserves_old_package_then_acceptance_updates_it(),
        auto_package_update_real_missing_archive_payload_reports_failure_and_preserves_installed_package(),
        auto_package_update_real_schedule_skips_before_interval_then_updates_at_due_boundary(),
    ]
}
