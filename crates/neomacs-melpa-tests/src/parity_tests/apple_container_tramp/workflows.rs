use expect_test::expect;

use super::ParityBatchCase;

fn apple_container_completion_discovers_live_hosts_with_the_selected_cli_context() -> ParityBatchCase
{
    ParityBatchCase::value(
        "apple_container_completion_discovers_live_hosts_with_the_selected_cli_context",
        r####"
(let* ((runtime
        (neomacs-apple-container-test-prepare
         "apple-container-completion"))
       (root (nth 0 runtime))
       (bin (nth 1 runtime))
       (calls (nth 2 runtime))
       (exec-path (cons bin exec-path))
       (process-environment
        (append
         (list
          (concat
           "APPLE_CONTAINER_TEST_LOG="
           calls)
          (concat
           "PATH="
           bin
           ":"
           (getenv "PATH")))
         process-environment))
       (apple-container-tramp-container-options
        '("--context" "development"))
       (tramp-persistency-file-name
        (expand-file-name
         "tramp-persistency.el"
         root))
       result)
  (unwind-protect
      (progn
        (apple-container-tramp-setup)
        (let ((executing-kbd-macro noninteractive)
              (confirm-nonexistent-file-or-buffer nil)
              (completion-styles '(basic))
              completion-category-defaults
              completion-category-overrides
              (payments-input
               (append
                "/container:pay"
                '(tab return)))
              (worker-input
               (append
                "/container:work"
                '(tab return)))
              payments
              worker)
          (setq unread-command-events payments-input
                payments
                (read-file-name
                 "Open container file: "
                 nil
                 nil
                 nil))
          (setq unread-command-events worker-input
                worker
                (read-file-name
                 "Open container file: "
                 nil
                 nil
                 nil))
          (setq result
                (list
                 :payments payments
                 :worker worker
                 :container-calls
                 (neomacs-apple-container-test-file-string
                  calls)))))
    (neomacs-apple-container-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:payments "/container:payments:" :worker "/container:worker:" :container-calls "--context development ls\n--context development ls\n--context development exec -it payments sh\n--context development ls\n--context development ls\n--context development exec -it worker sh\n")"#
        ]],
    )
    .fresh_process()
}

fn apple_container_supports_a_real_remote_edit_write_rename_and_directory_session()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apple_container_supports_a_real_remote_edit_write_rename_and_directory_session",
        r####"
(let* ((runtime
        (neomacs-apple-container-test-prepare
         "apple-container-remote-session"))
       (root (nth 0 runtime))
       (bin (nth 1 runtime))
       (calls (nth 2 runtime))
       (containers (nth 3 runtime))
       (container-root
        (expand-file-name
         "payments/"
         containers))
       (source
        (expand-file-name
         "workspace/config.txt"
         container-root))
       (remote
        (concat
         "/container:payments:"
         source))
       (remote-directory
        (file-name-directory remote))
       (remote-draft
        (expand-file-name
         "deploy-report.draft"
         remote-directory))
       (remote-report
        (expand-file-name
         "deploy-report.txt"
         remote-directory))
       (exec-path (cons bin exec-path))
       (process-environment
        (append
         (list
          (concat
           "APPLE_CONTAINER_TEST_LOG="
           calls)
          (concat
           "PATH="
           bin
           ":"
           (getenv "PATH")))
         process-environment))
       (apple-container-tramp-container-options
        '("--context" "development"))
       (tramp-persistency-file-name
        (expand-file-name
         "tramp-persistency.el"
         root))
       (tramp-verbose 0)
       (make-backup-files nil)
       buffer
       result)
  (unwind-protect
      (progn
        (make-directory
         (file-name-directory source)
         t)
        (with-temp-file source
          (insert
           "mode=development\n"
           "workers=2\n"))
        (apple-container-tramp-setup)
        (setq buffer
              (find-file-noselect remote))
        (with-current-buffer buffer
          (goto-char (point-min))
          (search-forward "workers=2")
          (replace-match "workers=4" t t)
          (save-buffer)
          (revert-buffer t t))
        (with-temp-buffer
          (insert
           "deployment=ready\n"
           "workers=4\n")
          (write-region
           (point-min)
           (point-max)
           remote-draft
           nil
           'silent))
        (rename-file
         remote-draft
         remote-report)
        (setq result
              (list
               :remote
               (list
                (file-remote-p remote)
                (file-remote-p remote 'user)
                (file-remote-p remote 'host)
                (file-remote-p remote 'localname))
               :buffer
               (with-current-buffer buffer
                 (list
                  (buffer-string)
                  (line-number-at-pos)
                  (current-column)
                  (buffer-modified-p)))
               :directory
               (directory-files
                remote-directory
                nil
                "\\`[^.]")
               :sizes
               (list
                (file-attribute-size
                 (file-attributes remote))
                (file-attribute-size
                 (file-attributes remote-report)))
               :config-on-disk
               (neomacs-apple-container-test-file-string
                source)
               :report-on-disk
               (neomacs-apple-container-test-file-string
                (expand-file-name
                 "workspace/deploy-report.txt"
                 container-root))
               :container-calls
               (neomacs-apple-container-test-file-string
                calls))))
    (when
        (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (neomacs-apple-container-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:remote ("/container:payments:" nil "payments" "[ORACLE-SANDBOX]/apple-container-remote-session/containers/payments/workspace/config.txt") :buffer ("mode=development\nworkers=4\n" 2 9 nil) :directory ("config.txt" "deploy-report.txt") :sizes (27 27) :config-on-disk "mode=development\nworkers=4\n" :report-on-disk "deployment=ready\nworkers=4\n" :container-calls "--context development exec -it payments sh\n")"#
        ]],
    )
    .fresh_process()
}

fn apple_container_remote_lock_round_trip_recognizes_the_connection_owner() -> ParityBatchCase {
    ParityBatchCase::value(
        "apple_container_remote_lock_round_trip_recognizes_the_connection_owner",
        r####"
(let* ((runtime
        (neomacs-apple-container-test-prepare
         "apple-container-lock-owner"))
       (root (nth 0 runtime))
       (bin (nth 1 runtime))
       (calls (nth 2 runtime))
       (containers (nth 3 runtime))
       (source
        (expand-file-name
         "payments/workspace/lock-owner.txt"
         containers))
       (remote
        (concat
         "/container:payments:"
         source))
       (exec-path (cons bin exec-path))
       (process-environment
        (append
         (list
          (concat
           "APPLE_CONTAINER_TEST_LOG="
           calls)
          (concat
           "PATH="
           bin
           ":"
           (getenv "PATH")))
         process-environment))
       (apple-container-tramp-container-options
        '("--context" "development"))
       (tramp-persistency-file-name
        (expand-file-name
         "tramp-persistency.el"
         root))
       (tramp-verbose 0)
       result)
  (unwind-protect
      (progn
        (make-directory
         (file-name-directory source)
         t)
        (with-temp-file source
          (insert "fixture\n"))
        (apple-container-tramp-setup)
        (lock-file remote)
        (let* ((info
                (tramp-get-lock-file remote))
               (matched
                (string-match
                 tramp-lock-file-info-regexp
                 info))
               (info-user
                (match-string 1 info))
               (info-host
                (match-string 2 info))
               (info-pid
                (match-string 3 info))
               (owner
                (file-locked-p remote)))
          (setq result
                (list
                 :format (integerp matched)
                 :user
                 (string-equal
                  info-user
                  (user-login-name))
                 :host
                 (string-equal
                  info-host
                  tramp-system-name)
                 :connection
                 (string-equal
                  info-pid
                  (tramp-get-lock-pid remote))
                 :owner owner)))
        (unlock-file remote)
        (setq result
              (append
               result
               (list
                :after-unlock
                (file-locked-p remote)))))
    (ignore-errors
      (unlock-file remote))
    (neomacs-apple-container-test-cleanup root))
  result)
"####,
        expect!["OK (:format t :user t :host t :connection t :owner t :after-unlock nil)"],
    )
    .fresh_process()
}

fn apple_container_cleanup_refreshes_remote_identity_without_disrupting_open_edits()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apple_container_cleanup_refreshes_remote_identity_without_disrupting_open_edits",
        r####"
(let* ((runtime
        (neomacs-apple-container-test-prepare
         "apple-container-cleanup"))
       (root (nth 0 runtime))
       (bin (nth 1 runtime))
       (calls (nth 2 runtime))
       (containers (nth 3 runtime))
       (payments-file
        (expand-file-name
         "payments/workspace/status.txt"
         containers))
       (retired-file
        (expand-file-name
         "retired/workspace/status.txt"
         containers))
       (payments-remote
        (concat
         "/container:root@payments:"
         payments-file))
       (retired-remote
        (concat
         "/container:root@retired:"
         retired-file))
       (exec-path (cons bin exec-path))
       (process-environment
        (append
         (list
          (concat
           "APPLE_CONTAINER_TEST_LOG="
           calls)
          (concat
           "PATH="
           bin
           ":"
           (getenv "PATH")))
         process-environment))
       (apple-container-tramp-container-options
        '("--context" "development"))
       (tramp-connection-properties
        `((,(concat
             "\\`"
             (regexp-quote
              "/container:root@retired:")
             "\\'")
           "uid-integer"
           42001)))
       (tramp-persistency-file-name
        (expand-file-name
         "tramp-persistency.el"
         root))
       (tramp-verbose 0)
       (make-backup-files nil)
       payments-buffer
       retired-buffer
       uid-before
       uid-stale
       uid-after
       result)
  (unwind-protect
      (progn
        (dolist
            (file
             (list
              payments-file
              retired-file))
          (make-directory
           (file-name-directory file)
           t)
          (with-temp-file file
            (insert "healthy\n")))
        (apple-container-tramp-setup)
        (setq payments-buffer
              (find-file-noselect payments-remote)
              retired-buffer
              (find-file-noselect retired-remote))
        (setq uid-before
              (let ((default-directory
                     (file-name-directory
                      retired-remote)))
                (file-user-uid)))
        (setq tramp-connection-properties nil)
        (setq uid-stale
              (let ((default-directory
                     (file-name-directory
                      retired-remote)))
                (file-user-uid)))
        (apple-container-tramp-cleanup)
        (setq uid-after
              (let ((default-directory
                     (file-name-directory
                      retired-remote)))
                (file-user-uid)))
        (dolist
            (entry
             `((,payments-buffer . "payments-checked\n")
               (,retired-buffer . "retired-checked\n")))
          (with-current-buffer (car entry)
            (goto-char (point-max))
            (insert (cdr entry))
            (save-buffer)
            (revert-buffer t t)))
        (setq result
              (list
               :remote-uid-cache
               (list
                :configured-before
                (= uid-before 42001)
                :stale-before-cleanup
                (= uid-stale uid-before)
                :refreshed-after-cleanup
                (/= uid-after uid-stale))
               :payments
               (with-current-buffer payments-buffer
                 (list
                  (buffer-string)
                  (buffer-modified-p)))
               :retired
               (with-current-buffer retired-buffer
                 (list
                  (buffer-string)
                  (buffer-modified-p)))
               :payments-on-disk
               (neomacs-apple-container-test-file-string
                payments-file)
               :retired-on-disk
               (neomacs-apple-container-test-file-string
                retired-file)
               :container-calls
               (neomacs-apple-container-test-file-string
                calls))))
    (dolist
        (buffer
         (list payments-buffer retired-buffer))
      (when
          (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))
    (neomacs-apple-container-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:remote-uid-cache (:configured-before t :stale-before-cleanup t :refreshed-after-cleanup t) :payments ("healthy\npayments-checked\n" nil) :retired ("healthy\nretired-checked\n" nil) :payments-on-disk "healthy\npayments-checked\n" :retired-on-disk "healthy\nretired-checked\n" :container-calls "--context development exec -it -u root payments sh\n--context development exec -it -u root retired sh\n--context development ls\n")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        apple_container_completion_discovers_live_hosts_with_the_selected_cli_context(),
        apple_container_remote_lock_round_trip_recognizes_the_connection_owner(),
        apple_container_supports_a_real_remote_edit_write_rename_and_directory_session(),
        apple_container_cleanup_refreshes_remote_identity_without_disrupting_open_edits(),
    ]
}
