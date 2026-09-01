use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_installer_prefix_default_uses_user_directory_or_no_littering_adapter() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_installer_prefix_default_uses_user_directory_or_no_littering_adapter",
        r##"(let ((user-emacs-directory
                    "/home/test/.emacs.d/")
                   calls)
               (list
                (cl-letf
                    (((symbol-function
                       'featurep)
                      (lambda (_)
                        nil)))
                  (asdf-vm-installer-prefix-default))
                (cl-letf
                    (((symbol-function
                       'featurep)
                      (lambda (feature)
                        (eq feature
                            'no-littering)))
                     ((symbol-function
                       'no-littering-expand-var-file-name)
                      (lambda (name)
                        (push name calls)
                        (concat
                         "/state/"
                         name))))
                  (asdf-vm-installer-prefix-default))
                calls))"##,
        expect![[r#"OK ("/home/test/.emacs.d/asdf" "/state/asdf" ("asdf"))"#]],
    )
}

fn asdf_vm_installer_system_and_architecture_detection_cover_overrides_platforms_and_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_system_and_architecture_detection_cover_overrides_platforms_and_errors",
        r##"(let (asdf-vm-installer-system
                    asdf-vm-installer-architecture)
               (list
                (mapcar
                 (lambda (configuration)
                   (let ((system-configuration
                          configuration))
                     (list
                      configuration
                      (asdf-vm-test-error-data
                       #'asdf-vm-installer--guess-system)
                      (asdf-vm-test-error-data
                       #'asdf-vm-installer--guess-architecture))))
                 '("x86_64-pc-linux-gnu"
                   "aarch64-apple-darwin"
                   "arm64-unknown-linux"
                   "i386-pc-linux"
                   "riscv64-unknown-freebsd"))
                (let ((asdf-vm-installer-system
                       "fixture-os")
                      (asdf-vm-installer-architecture
                       "fixture-cpu")
                      (system-configuration
                       "unsupported"))
                  (list
                   (asdf-vm-installer--guess-system)
                   (asdf-vm-installer--guess-architecture)))))"##,
        expect![[
            r#"OK ((("x86_64-pc-linux-gnu" (:ok "linux") (:ok "amd64")) ("aarch64-apple-darwin" (:ok "darwin") (:ok "arm64")) ("arm64-unknown-linux" (:ok "linux") (:ok "arm64")) ("i386-pc-linux" (:ok "linux") (:ok "386")) ("riscv64-unknown-freebsd" (:error asdf-vm-installer-unsupported-system ("riscv64-unknown-freebsd")) (:error asdf-vm-installer-unsupported-system ("riscv64-unknown-freebsd")))) ("fixture-os" "fixture-cpu"))"#
        ]],
    )
}

fn asdf_vm_installer_version_filter_enforces_minimum_and_rejects_invalid_versions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_version_filter_enforces_minimum_and_rejects_invalid_versions",
        r##"(mapcar
               (lambda (version)
                 (list
                  version
                  (asdf-vm-installer--version-filter
                   version)))
               '("0.15.9"
                 "0.16.0"
                 "0.16.0-rc1"
                 "0.16.1"
                 "1.0.0"
                 "v0.17.0"
                 "not-a-version"
                 ""
                 "資料"))"##,
        expect![[
            r#"OK (("0.15.9" nil) ("0.16.0" t) ("0.16.0-rc1" nil) ("0.16.1" t) ("1.0.0" t) ("v0.17.0" nil) ("not-a-version" nil) ("" nil) ("資料" nil))"#
        ]],
    )
}

fn asdf_vm_installer_remote_versions_parse_filter_order_and_memoize_git_output() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_installer_remote_versions_parse_filter_order_and_memoize_git_output",
        r##"(let* ((log
                     (asdf-vm-test-path
                      "git-fixture/arguments"))
                    (executable
                     (asdf-vm-test-make-executable
                      "git-fixture"
                      (concat
                       "for argument in \"$@\"; do "
                       "printf 'ARG=<%s>\\n' \"$argument\" >> \"$ASDF_VM_TEST_GIT_LOG\"; "
                       "done\n"
                       "printf 'aaa refs/tags/v0.15.9\\n'\n"
                       "printf 'bbb refs/tags/v0.16.0\\n'\n"
                       "printf 'ccc refs/tags/v0.16.2\\n'\n"
                       "printf 'ddd refs/tags/v1.0.0\\n'\n"
                       "printf 'eee refs/tags/vbad\\n'")))
                    (asdf-vm-installer-git-executable
                     executable)
                    (asdf-vm-installer-git-arguments
                     '("--fixture-global"))
                    (asdf-vm-installer-git-repo-url
                     "https://example/asdf.git")
                    (asdf-vm-installer--remote-version-list
                     nil))
               (setenv "ASDF_VM_TEST_GIT_LOG"
                       log)
               (make-directory
                (file-name-directory log)
                t)
               (let ((first
                      (asdf-vm-installer-list-all))
                     (second
                      (asdf-vm-installer-list-all)))
                 (list
                  first
                  second
                  asdf-vm-installer--remote-version-list
                  (asdf-vm-test-read-file
                   log))))"##,
        expect![[
            r#"OK (#1=("0.16.0" "0.16.2" "1.0.0") #1# #1# "ARG=<--fixture-global>\nARG=<ls-remote>\nARG=<--sort=v:refname>\nARG=<https://example/asdf.git>\nARG=<refs/tags/v*>\n")"#
        ]],
    )
}

fn asdf_vm_installer_list_all_internal_refetches_and_filters_each_real_git_invocation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_list_all_internal_refetches_and_filters_each_real_git_invocation",
        r##"(let* ((log
                     (asdf-vm-test-path
                      "git-internal/arguments"))
                    (executable
                     (asdf-vm-test-make-executable
                      "git-internal"
                      (concat
                       "for argument in \"$@\"; do "
                       "printf 'ARG=<%s>\\n' \"$argument\" >> \"$ASDF_VM_TEST_GIT_LOG\"; "
                       "done\n"
                       "printf 'aaa refs/tags/v0.15.9\\n'\n"
                       "printf 'bbb refs/tags/v0.16.0\\n'\n"
                       "printf 'ccc refs/tags/v0.17.1-rc1\\n'\n"
                       "printf 'ddd refs/tags/v2.0.0\\n'")))
                    (asdf-vm-installer-git-executable
                     executable)
                    (asdf-vm-installer-git-arguments
                     '("--fixture-global"))
                    (asdf-vm-installer-git-repo-url
                     "https://example/asdf.git"))
               (setenv "ASDF_VM_TEST_GIT_LOG"
                       log)
               (make-directory
                (file-name-directory log)
                t)
               (list
                (asdf-vm-installer-list-all-internal)
                (asdf-vm-installer-list-all-internal)
                (asdf-vm-test-read-file
                 log)))"##,
        expect![[
            r#"OK (("0.16.0" "0.17.1-rc1" "2.0.0") ("0.16.0" "0.17.1-rc1" "2.0.0") "ARG=<--fixture-global>\nARG=<ls-remote>\nARG=<--sort=v:refname>\nARG=<https://example/asdf.git>\nARG=<refs/tags/v*>\nARG=<--fixture-global>\nARG=<ls-remote>\nARG=<--sort=v:refname>\nARG=<https://example/asdf.git>\nARG=<refs/tags/v*>\n")"#
        ]],
    )
}

fn asdf_vm_installer_package_names_and_urls_cover_supported_platform_matrix() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_package_names_and_urls_cover_supported_platform_matrix",
        r##"(let ((asdf-vm-installer-github-url
                    "https://downloads.example/asdf"))
               (mapcar
                (lambda (spec)
                  (pcase-let
                      ((`(,version
                          ,system
                          ,architecture)
                        spec))
                    (list
                     spec
                     (asdf-vm-installer--package-name
                      version
                      system
                      architecture)
                     (asdf-vm-installer--package-url
                      version
                      system
                      architecture))))
                '(("0.16.0"
                   "linux"
                   "amd64")
                  ("0.17.2"
                   "darwin"
                   "arm64")
                  ("1.0.0-rc1"
                   "linux"
                   "386"))))"##,
        expect![[
            r#"OK ((("0.16.0" "linux" "amd64") "asdf-v0.16.0-linux-amd64.tar.gz" "https://downloads.example/asdf/releases/download/v0.16.0/asdf-v0.16.0-linux-amd64.tar.gz") (("0.17.2" "darwin" "arm64") "asdf-v0.17.2-darwin-arm64.tar.gz" "https://downloads.example/asdf/releases/download/v0.17.2/asdf-v0.17.2-darwin-arm64.tar.gz") (("1.0.0-rc1" "linux" "386") "asdf-v1.0.0-rc1-linux-386.tar.gz" "https://downloads.example/asdf/releases/download/v1.0.0-rc1/asdf-v1.0.0-rc1-linux-386.tar.gz"))"#
        ]],
    )
}

fn asdf_vm_installer_checksum_validation_reads_real_checksum_files_and_cli_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_checksum_validation_reads_real_checksum_files_and_cli_output",
        r##"(let* ((package
                     (asdf-vm-test-path
                      "checksum/asdf.tar.gz"))
                    (matching
                     (concat
                      package
                      ".md5"))
                    (mismatch
                     (asdf-vm-test-path
                      "checksum/wrong.md5"))
                    (asdf-vm-installer-md5sum-executable
                     (asdf-vm-test-make-executable
                      "md5sum-fixture"
                      "printf 'abc123  %s\\n' \"$1\""))
                    (asdf-vm-installer-md5sum-arguments
                     nil))
               (asdf-vm-test-write-file
                package "archive bytes")
               (asdf-vm-test-write-file
                matching "abc123\n")
               (asdf-vm-test-write-file
                mismatch "different\n")
               (list
                (asdf-vm-installer--valid-checksum-p
                 package)
                (asdf-vm-installer--valid-checksum-p
                 package mismatch)))"##,
        expect!["OK (t nil)"],
    )
}

fn asdf_vm_installer_download_builds_release_urls_writes_both_files_and_validates_checksum()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_download_builds_release_urls_writes_both_files_and_validates_checksum",
        r##"(let* ((asdf-vm-installer-src-dir
                     (asdf-vm-test-path
                      "download/src"))
                    (asdf-vm-installer-system
                     "linux")
                    (asdf-vm-installer-architecture
                     "amd64")
                    (asdf-vm-installer-github-url
                     "https://downloads.example/asdf")
                    calls)
               (cl-letf
                   (((symbol-function
                      'url-copy-file)
                     (lambda
                       (url path overwrite)
                       (push
                        (list
                         :copy url path overwrite)
                        calls)
                       (let ((file-name-handler-alist
                              nil))
                         (asdf-vm-test-write-file
                          path
                          (if
                              (string-suffix-p
                               ".md5"
                               path)
                              "checksum"
                            "archive")))
                       nil))
                    ((symbol-function
                      'asdf-vm-installer--valid-checksum-p)
                     (lambda
                       (path checksum)
                       (push
                        (list
                         :checksum
                         path
                         checksum
                         (let ((file-name-handler-alist
                                nil))
                           (asdf-vm-test-read-file
                            path))
                         (let ((file-name-handler-alist
                                nil))
                           (asdf-vm-test-read-file
                            checksum)))
                        calls)
                       t)))
                 (list
                  (asdf-vm-installer-download
                   "0.16.2" 1)
                  (sort
                   (directory-files-recursively
                    asdf-vm-installer-src-dir
                    ".*")
                   #'string<)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("[asdf-vm] version 0.16.2 downloaded" ("[ORACLE-SANDBOX]/download/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz" "[ORACLE-SANDBOX]/download/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz.md5") ((:copy "https://downloads.example/asdf/releases/download/v0.16.2/asdf-v0.16.2-linux-amd64.tar.gz" "[ORACLE-SANDBOX]/download/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz" t) (:copy "https://downloads.example/asdf/releases/download/v0.16.2/asdf-v0.16.2-linux-amd64.tar.gz.md5" "[ORACLE-SANDBOX]/download/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz.md5" t) (:checksum "[ORACLE-SANDBOX]/download/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz" "[ORACLE-SANDBOX]/download/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz.md5" "archive" "checksum")))"#
        ]],
    )
}

fn asdf_vm_installer_download_checksum_mismatch_deletes_payload_and_checksum_then_signals()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_download_checksum_mismatch_deletes_payload_and_checksum_then_signals",
        r##"(let* ((asdf-vm-installer-src-dir
                     (asdf-vm-test-path
                      "download-mismatch/src"))
                    (asdf-vm-installer-system
                     "linux")
                    (asdf-vm-installer-architecture
                     "amd64")
                    copied)
               (cl-letf
                   (((symbol-function
                      'url-copy-file)
                     (lambda
                       (_url path _overwrite)
                       (push path copied)
                       (asdf-vm-test-write-file
                        path "downloaded")
                       nil))
                    ((symbol-function
                      'asdf-vm-installer--valid-checksum-p)
                     (lambda (&rest _)
                       nil)))
                 (let ((result
                        (asdf-vm-test-error-data
                         (lambda ()
                           (asdf-vm-installer-download
                            "0.16.2")))))
                   (list
                    result
                    (mapcar
                     (lambda (path)
                       (list
                        path
                        (file-exists-p path)))
                     (nreverse copied))))))"##,
        expect![[
            r#"OK ((:error asdf-vm-installer-checksum-mismatch nil) (("[ORACLE-SANDBOX]/download-mismatch/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz" nil) ("[ORACLE-SANDBOX]/download-mismatch/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz.md5" nil)))"#
        ]],
    )
}

fn asdf_vm_installer_install_downloads_missing_archive_extracts_and_cleanup_hook_removes_downloads()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_install_downloads_missing_archive_extracts_and_cleanup_hook_removes_downloads",
        r##"(let* ((asdf-vm-installer-src-dir
                     (asdf-vm-test-path
                      "install/src"))
                    (asdf-vm-installer-system
                     "linux")
                    (asdf-vm-installer-architecture
                     "amd64")
                    (kill-emacs-hook nil)
                    (tar-log
                     (asdf-vm-test-path
                      "install/tar-arguments"))
                    (asdf-vm-installer-tar-executable
                     (asdf-vm-test-make-executable
                      "tar-fixture"
                      (concat
                       "for argument in \"$@\"; do "
                       "printf 'ARG=<%s>\\n' \"$argument\" >> \"$ASDF_VM_TEST_TAR_LOG\"; "
                       "done")))
                    (asdf-vm-installer-tar-arguments
                     '("--fixture-global"))
                    calls)
               (setenv "ASDF_VM_TEST_TAR_LOG"
                       tar-log)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-installer-download)
                     (lambda
                       (version interactive)
                       (let* ((directory
                               (expand-file-name
                                version
                                asdf-vm-installer-src-dir))
                              (package
                               (expand-file-name
                                (asdf-vm-installer--package-name
                                 version)
                                directory)))
                         (push
                          (list
                           :download
                           version interactive)
                          calls)
                         (asdf-vm-test-write-file
                          package "archive")
                         (asdf-vm-test-write-file
                          (concat
                           package
                           ".md5")
                          "checksum")))))
                 (let* ((version
                         "0.16.2")
                        (directory
                         (expand-file-name
                          version
                          asdf-vm-installer-src-dir))
                        (package
                         (expand-file-name
                          (asdf-vm-installer--package-name
                           version)
                          directory))
                        (result
                         (asdf-vm-installer-install
                          version nil 1))
                        (before
                         (list
                          (file-exists-p package)
                          (file-exists-p
                           (concat
                            package
                            ".md5"))
                          (length
                           kill-emacs-hook))))
                   (run-hooks
                    'kill-emacs-hook)
                   (list
                    result
                    before
                    (file-exists-p package)
                    (file-exists-p
                     (concat
                      package
                      ".md5"))
                    (asdf-vm-test-read-file
                     tar-log)
                    (nreverse calls)))))"##,
        expect![[
            r#"OK ("[asdf-vm] version 0.16.2 installed" (t t 1) nil nil "ARG=<--fixture-global>\nARG=<--extract>\nARG=<--file=[ORACLE-SANDBOX]/install/src/0.16.2/asdf-v0.16.2-linux-amd64.tar.gz>\nARG=<--directory=[ORACLE-SANDBOX]/install/src/0.16.2>\n" ((:download "0.16.2" 1)))"#
        ]],
    )
}

fn asdf_vm_installer_list_filters_real_directories_and_activate_replaces_symlink() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_installer_list_filters_real_directories_and_activate_replaces_symlink",
        r##"(let* ((asdf-vm-installer-src-dir
                     (asdf-vm-test-path
                      "installed/src"))
                    (asdf-vm-installer-bin-dir
                     (asdf-vm-test-path
                      "installed/bin"))
                    (first
                     (expand-file-name
                      "0.16.0"
                      asdf-vm-installer-src-dir))
                    (second
                     (expand-file-name
                      "0.17.2"
                      asdf-vm-installer-src-dir)))
               (make-directory first t)
               (make-directory second t)
               (make-directory
                (expand-file-name
                 "0.15.9"
                 asdf-vm-installer-src-dir)
                t)
               (make-directory
                (expand-file-name
                 "not-a-version"
                 asdf-vm-installer-src-dir)
                t)
               (asdf-vm-test-write-file
                (expand-file-name
                 "asdf"
                 first)
                "first")
               (asdf-vm-test-write-file
                (expand-file-name
                 "asdf"
                 second)
                "second")
               (let ((versions
                      (asdf-vm-installer-list)))
                 (asdf-vm-installer-activate
                  "0.16.0")
                 (let ((first-target
                        (file-symlink-p
                         (expand-file-name
                          "asdf"
                          asdf-vm-installer-bin-dir))))
                   (asdf-vm-installer-activate
                    "0.17.2")
                   (list
                    versions
                    first-target
                    (file-symlink-p
                     (expand-file-name
                      "asdf"
                      asdf-vm-installer-bin-dir))))))"##,
        expect![[
            r#"OK (("0.16.0" "0.17.2") "[ORACLE-SANDBOX]/installed/src/0.16.0/asdf" "[ORACLE-SANDBOX]/installed/src/0.17.2/asdf")"#
        ]],
    )
}

fn asdf_vm_installer_list_internal_filters_version_names_without_assuming_entry_kind()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_list_internal_filters_version_names_without_assuming_entry_kind",
        r##"(let ((asdf-vm-installer-src-dir
                    (asdf-vm-test-path
                     "list-internal/src")))
               (make-directory
                (expand-file-name
                 "0.16.0"
                 asdf-vm-installer-src-dir)
                t)
               (make-directory
                (expand-file-name
                 "not-a-version"
                 asdf-vm-installer-src-dir)
                t)
               (asdf-vm-test-write-file
                (expand-file-name
                 "0.17.2"
                 asdf-vm-installer-src-dir)
                "ordinary file with version-shaped name")
               (make-symbolic-link
                (expand-file-name
                 "0.16.0"
                 asdf-vm-installer-src-dir)
                (expand-file-name
                 "1.0.0"
                 asdf-vm-installer-src-dir))
               (asdf-vm-installer-list-internal))"##,
        expect![[r#"OK ("0.16.0" "0.17.2" "1.0.0")"#]],
    )
}

fn asdf_vm_installer_orchestrates_missing_and_existing_versions_then_selects_binary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_installer_orchestrates_missing_and_existing_versions_then_selects_binary",
        r##"(let ((asdf-vm-installer-bin-dir
                    "/fixture/bin")
                   installed
                   calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-installer--installed-p)
                     (lambda (version)
                       (push
                        (list
                         :installed-p
                         version installed)
                        calls)
                       installed))
                    ((symbol-function
                      'asdf-vm-installer-install)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :install arguments)
                        calls)
                       :installed))
                    ((symbol-function
                      'asdf-vm-installer-activate)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :activate arguments)
                        calls)
                       :activated)))
                 (let ((first
                        (asdf-vm-installer
                         "0.16.2" t 4))
                       (first-executable
                        asdf-vm-process-executable))
                   (setq installed t)
                   (let ((second
                          (asdf-vm-installer
                           "0.16.2" nil 1)))
                     (list
                      first
                      first-executable
                      second
                      asdf-vm-process-executable
                      (nreverse calls))))))"##,
        expect![[
            r#"OK ("/fixture/bin/asdf" "/fixture/bin/asdf" "/fixture/bin/asdf" "/fixture/bin/asdf" ((:installed-p "0.16.2" nil) (:install "0.16.2" t 4) (:activate "0.16.2" 4) (:installed-p "0.16.2" t) (:activate "0.16.2" 1)))"#
        ]],
    )
}

pub(super) fn installer_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_installer_prefix_default_uses_user_directory_or_no_littering_adapter(),
        asdf_vm_installer_system_and_architecture_detection_cover_overrides_platforms_and_errors(),
        asdf_vm_installer_version_filter_enforces_minimum_and_rejects_invalid_versions(),
        asdf_vm_installer_remote_versions_parse_filter_order_and_memoize_git_output(),
        asdf_vm_installer_list_all_internal_refetches_and_filters_each_real_git_invocation(),
        asdf_vm_installer_package_names_and_urls_cover_supported_platform_matrix(),
        asdf_vm_installer_checksum_validation_reads_real_checksum_files_and_cli_output(),
        asdf_vm_installer_download_builds_release_urls_writes_both_files_and_validates_checksum(),
        asdf_vm_installer_download_checksum_mismatch_deletes_payload_and_checksum_then_signals(),
        asdf_vm_installer_install_downloads_missing_archive_extracts_and_cleanup_hook_removes_downloads(),
        asdf_vm_installer_list_filters_real_directories_and_activate_replaces_symlink(),
        asdf_vm_installer_list_internal_filters_version_names_without_assuming_entry_kind(),
        asdf_vm_installer_orchestrates_missing_and_existing_versions_then_selects_binary(),
    ]
}
