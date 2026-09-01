use expect_test::expect;

use super::ParityBatchCase;

fn status_links_store_through_org_and_reopen_the_same_repository() -> ParityBatchCase {
    ParityBatchCase::value(
        "status_links_store_through_org_and_reopen_the_same_repository",
        r##"
(neomacs-orgit-test-run
 "status links λ"
 (lambda (root _file first second)
   (let ((default-directory root)
         (org-stored-links nil)
         status-buffer reopened)
     (unwind-protect
         (progn
           (setq status-buffer (magit-status-setup-buffer root))
           (with-current-buffer status-buffer
             (call-interactively #'org-store-link))
           (let ((stored (car org-stored-links)))
             (kill-buffer status-buffer)
             (setq status-buffer nil)
             (setq reopened
                   (orgit-status-open
                    (string-remove-prefix "orgit:" (car stored))))
             (neomacs-orgit-test-normalize
              (list :stored stored
                    :mode (buffer-local-value 'major-mode reopened)
                    :toplevel
                    (with-current-buffer reopened (magit-toplevel))
                    :head
                    (with-current-buffer reopened
                      (magit-rev-parse "HEAD")))
              root first second)))
       (when (buffer-live-p status-buffer) (kill-buffer status-buffer))
       (when (buffer-live-p reopened) (kill-buffer reopened))))))
"##,
        expect![[
            r#"OK (:stored ("orgit:<REPO>/" "<REPO>/ (magit-status)") :mode magit-status-mode :toplevel "<REPO>/" :head "<SECOND>")"#
        ]],
    )
}

fn revision_links_preserve_commit_reference_and_search_text_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "revision_links_preserve_commit_reference_and_search_text_forms",
        r##"
(neomacs-orgit-test-run
 "revision links"
 (lambda (root _file first second)
   (let ((default-directory root)
         (org-stored-links nil)
         revision-buffer)
     (unwind-protect
         (progn
           (neomacs-orgit-test-git root "tag" "release-λ" second)
           (setq revision-buffer
                 (magit-revision-setup-buffer
                  second
                  (car (magit-diff-arguments 'magit-revision-mode))
                  nil))
           (with-current-buffer revision-buffer
             (let ((current-prefix-arg nil)
                   (orgit-store-reference nil))
               (call-interactively #'org-store-link))
             (let ((current-prefix-arg '(4))
                   (orgit-store-reference nil))
               (call-interactively #'org-store-link))
             (let ((current-prefix-arg '-)
                   (orgit-store-reference nil))
               (call-interactively #'org-store-link)))
           (neomacs-orgit-test-normalize
            (list :stored (reverse org-stored-links)
                  :count (length org-stored-links))
            root first second))
       (when (buffer-live-p revision-buffer)
         (kill-buffer revision-buffer))))))
"##,
        expect![[
            r#"OK (:stored (("orgit-rev:<REPO>/::<SECOND>" "<REPO>/ (magit-rev <SECOND7>)") ("orgit-rev:<REPO>/::release-λ" "<REPO>/ (magit-rev release-λ)") ("orgit-rev:<REPO>/:::/Document Orgit workflow" "<REPO>/ (magit-rev :/Document Orgit workflow)")) :count 3)"#
        ]],
    )
}

fn log_links_round_trip_revisions_arguments_and_file_limits() -> ParityBatchCase {
    ParityBatchCase::value(
        "log_links_round_trip_revisions_arguments_and_file_limits",
        r##"
(neomacs-orgit-test-run
 "log round trip"
 (lambda (root file first second)
   (let ((default-directory root)
         (org-stored-links nil)
         (relative (file-relative-name file root))
         log-buffer reopened)
     (unwind-protect
         (progn
           (setq log-buffer
                 (magit-log-setup-buffer
                  '("main" "main~1")
                  '("--graph" "--decorate" "-n5")
                  (list relative)))
           (with-current-buffer log-buffer
             (let ((orgit-log-save-arguments t))
               (call-interactively #'org-store-link)))
           (let* ((stored (car org-stored-links))
                  (path (string-remove-prefix "orgit-log:" (car stored))))
             (kill-buffer log-buffer)
             (setq log-buffer nil)
             (setq reopened (orgit-log-open path))
             (neomacs-orgit-test-normalize
              (list :stored stored
                    :revisions
                    (buffer-local-value 'magit-buffer-log-revisions reopened)
                    :arguments
                    (buffer-local-value 'magit-buffer-log-args reopened)
                    :files
                    (buffer-local-value 'magit-buffer-log-files reopened)
                    :mode (buffer-local-value 'major-mode reopened))
              root first second)))
       (when (buffer-live-p log-buffer) (kill-buffer log-buffer))
       (when (buffer-live-p reopened) (kill-buffer reopened))))))
"##,
        expect![[
            r#"OK (:stored ("orgit-log:<REPO>/::((\"main\" \"main~1\") (\"--graph\" \"--decorate\" \"-n5\") (\"docs/release λ notes.txt\"))" "<REPO>/ (magit-log (\"main\" \"main~1\") (\"--graph\" \"--decorate\" \"-n5\") (\"docs/release λ notes.txt\"))") :revisions ("main" "main~1") :arguments ("--graph" "--decorate" "-n5") :files ("docs/release λ notes.txt") :mode magit-log-mode)"#
        ]],
    )
}

fn blob_links_restore_unicode_file_line_ranges_and_columns() -> ParityBatchCase {
    ParityBatchCase::value(
        "blob_links_restore_unicode_file_line_ranges_and_columns",
        r##"
(neomacs-orgit-test-run
 "blob links"
 (lambda (root file first second)
   (let ((default-directory root)
         (org-stored-links nil)
         (relative (file-relative-name file root))
         blob-buffer reopened)
     (unwind-protect
         (progn
           (setq blob-buffer (magit-find-file "HEAD" relative))
           (with-current-buffer blob-buffer
             (goto-char (point-min))
             (forward-line 1)
             (set-mark (point))
             (forward-line 2)
             (activate-mark)
             (call-interactively #'org-store-link))
           (let* ((range-link (car org-stored-links))
                  (range-path
                   (string-remove-prefix "orgit-blob:" (car range-link))))
             (setq org-stored-links nil)
             (with-current-buffer blob-buffer
               (deactivate-mark)
               (goto-char (point-min))
               (forward-line 3)
               (forward-char 1)
               (call-interactively #'org-store-link))
             (let* ((column-link (car org-stored-links))
                    (column-path
                     (string-remove-prefix "orgit-blob:"
                                           (car column-link))))
               (kill-buffer blob-buffer)
               (setq blob-buffer nil)
               (orgit-blob-open range-path)
               (setq reopened (magit-find-file-noselect second relative))
               (let ((range-state
                      (with-current-buffer reopened
                        (list :line (line-number-at-pos)
                              :mark-line
                              (and (mark t) (line-number-at-pos (mark t)))
                              :region
                              (and (mark t)
                                   (buffer-substring-no-properties
                                    (min (point) (mark t))
                                    (max (point) (mark t)))))))
                     column-state)
                 (kill-buffer reopened)
                 (setq reopened nil)
                 (orgit-blob-open column-path)
                 (setq reopened (magit-find-file-noselect second relative))
                 (setq column-state
                       (with-current-buffer reopened
                         (list :line (line-number-at-pos)
                               :column (current-column)
                               :text (buffer-substring-no-properties
                                      (line-beginning-position)
                                      (line-end-position)))))
                 (neomacs-orgit-test-normalize
                  (list :range-link range-link
                        :range-state range-state
                        :column-link column-link
                        :column-state column-state)
                  root first second)))))
       (when (buffer-live-p blob-buffer) (kill-buffer blob-buffer))
       (when (buffer-live-p reopened) (kill-buffer reopened))))))
"##,
        expect![[
            r#"OK (:range-link ("orgit-blob:<REPO>/::<SECOND>/docs/release λ notes.txt#2-4" "<REPO>/:<SECOND7>:docs/release λ notes.txt") :range-state (:line 4 :mark-line 2 :region "bravo λ\ncharlie\n") :column-link ("orgit-blob:<REPO>/::<SECOND>/docs/release λ notes.txt#4,2" "<REPO>/:<SECOND7>:docs/release λ notes.txt") :column-state (:line 4 :column 4 :text "四行目"))"#
        ]],
    )
}

fn remote_exports_cover_html_ascii_custom_urls_and_host_line_anchors() -> ParityBatchCase {
    ParityBatchCase::value(
        "remote_exports_cover_html_ascii_custom_urls_and_host_line_anchors",
        r##"
(neomacs-orgit-test-run
 "remote export"
 (lambda (root file first second)
   (let* ((default-directory root)
          (relative (file-relative-name file root))
          (blob-path
           (format "%s::%s/%s#2-4" root second relative)))
     (neomacs-orgit-test-git
      root "remote" "add" "origin"
      "git@github.com:team/project-λ.git")
     (let ((github
            (list
             (orgit-status-export root nil 'ascii nil)
             (orgit-log-export
              (format "%s::main" root) nil 'html nil)
             (orgit-rev-export
              (format "%s::%s" root second) nil 'ascii nil)
             (orgit-blob-export blob-path nil 'html nil))))
       (neomacs-orgit-test-git
        root "config" "orgit.rev"
        "https://review.example.test/changes/%r")
       (let ((custom
              (orgit-rev-export
               (format "%s::%s" root first) nil 'latex nil)))
         (neomacs-orgit-test-normalize
          (list :github github :custom custom)
          root first second))))))
"##,
        expect![[
            r#"OK (:github ("https://github.com/team/project-λ" "<a href=\"https://github.com/team/project-λ/commits/main\">https://github.com/team/project-λ/commits/main</a>" "https://github.com/team/project-λ/commit/<SECOND>" "<a href=\"https://github.com/team/project-λ/blob/<SECOND>/docs/release λ notes.txt?plain=1#L2-L4\">https://github.com/team/project-λ/blob/<SECOND>/docs/release λ notes.txt?plain=1#L2-L4</a>") :custom "\\href{https://review.example.test/changes/<FIRST>}{https://review.example.test/changes/<FIRST>}")"#
        ]],
    )
}

fn repository_ids_resolve_shared_links_and_fail_cleanly_when_unknown() -> ParityBatchCase {
    ParityBatchCase::value(
        "repository_ids_resolve_shared_links_and_fail_cleanly_when_unknown",
        r##"
(neomacs-orgit-test-run
 "repository ids/worktree"
 (lambda (root _file first second)
   (let* ((parent (file-name-directory (directory-file-name root)))
          (default-directory root)
          (magit-repository-directories (list (cons parent 1)))
          (orgit-store-repository-id t)
          (id (orgit--current-repository))
          (resolved (orgit--repository-directory id))
          (missing
           (neomacs-orgit-test-outcome
            (lambda () (orgit-status-open "not-checked-out")))))
     (neomacs-orgit-test-normalize
      (list :id id
            :resolved resolved
            :status-link
            (cl-letf (((symbol-function 'magit-read-repository)
                       (lambda (&optional _read-directory-name) root)))
              (orgit-status-complete-link))
            :missing missing)
      root first second))))
"##,
        expect![[
            r#"OK (:id "worktree" :resolved "<REPO>/" :status-link "orgit:worktree" :missing (:signal error :data ("Cannot open link; no entry for \"not-checked-out\" in ‘magit-repository-directories’") :message "Cannot open link; no entry for \"not-checked-out\" in ‘magit-repository-directories’"))"#
        ]],
    )
}

fn broken_exports_distinguish_missing_repositories_remotes_and_host_templates() -> ParityBatchCase {
    ParityBatchCase::value(
        "broken_exports_distinguish_missing_repositories_remotes_and_host_templates",
        r##"
(neomacs-orgit-test-run
 "broken exports"
 (lambda (root _file first second)
   (let ((default-directory root))
     (let ((no-remote
            (neomacs-orgit-test-outcome
             (lambda () (orgit-status-export root nil 'ascii nil)))))
       (neomacs-orgit-test-git
        root "remote" "add" "origin"
        "ssh://git@example.test/team/project.git")
       (let ((unknown-host
              (neomacs-orgit-test-outcome
               (lambda () (orgit-status-export root nil 'ascii nil))))
             (missing-repository
              (neomacs-orgit-test-outcome
               (lambda ()
                 (orgit-rev-export
                  (format "%s::HEAD"
                          (expand-file-name "missing/" root))
                  nil 'ascii nil)))))
         (neomacs-orgit-test-normalize
          (list :no-remote no-remote
                :unknown-host unknown-host
                :missing-repository missing-repository)
          root first second))))))
"##,
        expect![[
            r#"OK (:no-remote (:signal org-link-broken :data ("Cannot determine public remote for <REPO>/") :message "Unable to resolve link; aborting: \"Cannot determine public remote for <REPO>/\"") :unknown-host (:signal org-link-broken :data ("Cannot determine public url for <REPO>/") :message "Unable to resolve link; aborting: \"Cannot determine public url for <REPO>/\"") :missing-repository (:signal org-link-broken :data ("Cannot determine public url for <REPO>/missing/::HEAD (which itself does not exist)") :message "Unable to resolve link; aborting: \"Cannot determine public url for <REPO>/missing/::HEAD (which itself does not exist)\""))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        status_links_store_through_org_and_reopen_the_same_repository(),
        revision_links_preserve_commit_reference_and_search_text_forms(),
        log_links_round_trip_revisions_arguments_and_file_limits(),
        blob_links_restore_unicode_file_line_ranges_and_columns(),
        remote_exports_cover_html_ascii_custom_urls_and_host_line_anchors(),
        repository_ids_resolve_shared_links_and_fail_cleanly_when_unknown(),
        broken_exports_distinguish_missing_repositories_remotes_and_host_templates(),
    ]
}
