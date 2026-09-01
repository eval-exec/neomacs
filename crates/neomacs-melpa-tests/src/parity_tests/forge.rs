use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FORGE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(setq forge-add-default-sections nil
      forge-add-default-bindings nil)
(defvar neomacs-forge-test-original-max-lisp-eval-depth
  max-lisp-eval-depth)
(setq max-lisp-eval-depth 10000)
(require 'forge)
(setq max-lisp-eval-depth
      neomacs-forge-test-original-max-lisp-eval-depth)
(require 'cl-lib)
(let ((load-suffixes (append load-suffixes (list module-file-suffix))))
  (require 'sqlite3))

(setq magit-git-global-arguments
      (append
       '("-c" "init.defaultBranch=master"
         "-c" "user.name=Release Bot"
         "-c" "user.email=release-bot@example.test")
       magit-git-global-arguments))

(defun neomacs-forge-test-plain (value)
  "Remove text properties recursively from VALUE."
  (cond
   ((stringp value) (substring-no-properties value))
   ((vectorp value)
    (mapcar #'neomacs-forge-test-plain value))
   ((consp value)
    (cons (neomacs-forge-test-plain (car value))
          (neomacs-forge-test-plain (cdr value))))
   (t value)))

(defun neomacs-forge-test-git (root &rest args)
  "Run Git ARGS in ROOT and return trimmed output."
  (let ((default-directory (file-name-as-directory root)))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil args)))
        (unless (zerop status)
          (error "git %S failed (%s): %s" args status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-forge-test-close-database ()
  "Close Forge's singleton connection when it is live."
  (when-let ((database (forge-db t)))
    (emacsql-close database)))

(defun neomacs-forge-test-open-workspace (&optional tracked)
  "Create a real Git workspace and optionally TRACK it in Forge's database."
  (neomacs-forge-test-close-database)
  (let* ((root (make-temp-file "forge-workspace-" t))
         (default-directory (file-name-as-directory root)))
    (setq forge-database-file (expand-file-name "state/forge.sqlite" root))
    (neomacs-forge-test-git root "init" "-q")
    (neomacs-forge-test-git root "config" "user.name" "Release Bot")
    (neomacs-forge-test-git
     root "config" "user.email" "release-bot@example.test")
    (with-temp-file (expand-file-name "README.md" root)
      (insert "# Release Console\n\nTracks deployments.\n"))
    (neomacs-forge-test-git root "add" "README.md")
    (neomacs-forge-test-git root "commit" "-q" "-m" "Initial release console")
    (neomacs-forge-test-git
     root "remote" "add" "origin" "git@github.com:acme/release-console.git")
    (neomacs-forge-test-git
     root "update-ref" "refs/remotes/origin/master" "HEAD")
    (let ((repo (forge-get-repository :stub)))
      (when tracked
        (oset repo condition :tracked)
        (closql-insert (forge-db) repo t))
      (list :root root :repo repo))))

(defun neomacs-forge-test-clean-workspace (workspace)
  "Close database and remove all buffers and files owned by WORKSPACE."
  (let ((root (plist-get workspace :root)))
    (dolist (buffer (buffer-list))
      (with-current-buffer buffer
        (when (or (and buffer-file-name
                       (file-in-directory-p buffer-file-name root))
                  (and default-directory
                       (file-in-directory-p default-directory root)))
          (set-buffer-modified-p nil)
          (kill-buffer buffer))))
    (neomacs-forge-test-close-database)
    (delete-directory root t)))

(defun neomacs-forge-test-seed-repository (repo)
  "Persist realistic repository, issue, pull-request, and comment data."
  (let* ((bug-id (base64-encode-string "Label:bug" t))
         (release-id (base64-encode-string "Label:release" t))
         (alice-id (base64-encode-string "User:alice" t))
         (bob-id (base64-encode-string "User:bob" t)))
    (forge--update-repository
     repo
     '((createdAt . "2026-07-01T09:00:00Z")
       (updatedAt . "2026-07-31T22:55:00Z")
       (pushedAt . "2026-07-31T22:50:00Z")
       (parent . nil)
       (description . "Release orchestration Ω")
       (homepageUrl . "https://release.example.test")
       (defaultBranchRef (name . "master"))
       (isArchived . nil) (isFork . nil) (isLocked . nil)
       (isMirror . nil) (isPrivate . t)
       (hasIssuesEnabled . t) (hasDiscussionsEnabled . t)
       (hasWikiEnabled . nil)
       (stargazers (totalCount . 128))
       (watchers (totalCount . 17))
       (owner (teams . nil))))
    (forge--update-labels
     repo
     `(((id . ,bug-id) (name . "bug") (color . "d73a4a")
        (description . "Something is broken"))
       ((id . ,release-id) (name . "release") (color . "0052cc")
        (description . "Release train work"))))
    (forge--update-assignees
     repo
     `(((id . ,alice-id) (login . "alice") (name . "Alice Ops"))
       ((id . ,bob-id) (login . "bob") (name . "Bob Reviewer"))))
    (let* ((issue
            (forge--update-issue
             repo
             `((number . 42)
               (id . ,(base64-encode-string "Issue:42" t))
               (state . "OPEN") (stateReason . nil)
               (isReadByViewer . nil)
               (author (login . "alice"))
               (title . "Deployment fails for 東京 region")
               (createdAt . "2026-07-20T10:00:00Z")
               (updatedAt . "2026-07-31T21:30:00Z")
               (closedAt . nil) (locked . nil) (milestone . nil)
               (body . "Reproduce with `deploy --region 東京`.\r\nSecond line.")
               (assignees ((id . ,alice-id)))
               (labels ((id . ,bug-id)) ((id . ,release-id)))
               (comments
                ((id . ,(base64-encode-string "Comment:4201" t))
                 (databaseId . 4201)
                 (author (login . "bob"))
                 (createdAt . "2026-07-31T20:00:00Z")
                 (updatedAt . "2026-07-31T20:05:00Z")
                 (body . "Confirmed on canary Ω."))))
             t nil))
           (pullreq
            (forge--update-pullreq
             repo
             `((number . 17)
               (id . ,(base64-encode-string "PullRequest:17" t))
               (state . "OPEN") (isReadByViewer . t)
               (author (login . "bob"))
               (title . "Retry regional deployment atomically")
               (createdAt . "2026-07-25T08:00:00Z")
               (updatedAt . "2026-07-31T22:00:00Z")
               (closedAt . nil) (mergedAt . nil)
               (isDraft . t) (locked . nil)
               (maintainerCanModify . t) (isCrossRepository . t)
               (baseRef (name . "master")
                        (repository (nameWithOwner . "acme/release-console")))
               (baseRefOid . "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
               (headRef (name . "retry-region")
                        (repository
                         (nameWithOwner . "contributors/release-console")
                         (owner (login . "bob"))))
               (headRefOid . "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
               (milestone . nil)
               (body . "Makes publish and rollback one transaction.")
               (assignees ((id . ,alice-id)))
               (reviewRequests ((requestedReviewer (id . ,bob-id))))
               (labels ((id . ,release-id)))
               (comments
                ((id . ,(base64-encode-string "Comment:1701" t))
                 (databaseId . 1701)
                 (author (login . "alice"))
                 (createdAt . "2026-07-31T22:10:00Z")
                 (updatedAt . "2026-07-31T22:10:00Z")
                 (body . "Canary is green; ready for review."))))
             t nil)))
      (list :issue issue :pullreq pullreq))))
"####;

fn detects_a_real_git_remote_and_formats_repository_links() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((workspace (neomacs-forge-test-open-workspace))
       (root (plist-get workspace :root))
       (repo (plist-get workspace :repo))
       (default-directory (file-name-as-directory root)))
  (unwind-protect
      (let ((https-repo
             (forge-get-repository
              "https://github.com/acme/release-console.git" nil :stub)))
        (list
         :remote (oref repo remote)
         :identity
         (list :class (eieio-object-class-name repo)
               :condition (oref repo condition)
               :forge (oref repo forge)
               :githost (oref repo githost)
               :apihost (oref repo apihost)
               :owner (oref repo owner)
               :name (oref repo name)
               :worktree (equal (file-name-as-directory root)
                                (file-name-as-directory (oref repo worktree))))
         :protocols
         (mapcar #'forge--split-forge-url
                 '("git@github.com:acme/release-console.git"
                   "ssh://git@github.com/acme/release-console.git"
                   "https://github.com/acme/release-console"
                   "git://github.com/acme/release-console.git"))
         :same-repository (forge-repository-equal repo https-repo)
         :links
         (list (forge-get-url repo)
               (forge--format repo 'issues-url-format)
               (forge--format repo 'pullreqs-url-format)
               (forge--format repo 'issue-url-format '((?i . 42)))
               (forge--format repo 'pullreq-url-format '((?i . 17)))
               (forge--format repo 'commit-url-format
                              '((?r . "deadbeef"))))))
    (neomacs-forge-test-clean-workspace workspace)))
"####;
    let expected = expect![[
        r#"OK (:remote "origin" :identity (:class forge-github-repository :condition :stub :forge "github.com" :githost "github.com" :apihost "api.github.com" :owner "acme" :name "release-console" :worktree t) :protocols (("github.com" "acme" "release-console") ("github.com" "acme" "release-console") ("github.com" "acme" "release-console") ("github.com" "acme" "release-console")) :same-repository t :links ("https://github.com/acme/release-console" "https://github.com/acme/release-console/issues" "https://github.com/acme/release-console/pulls" "https://github.com/acme/release-console/issues/42" "https://github.com/acme/release-console/pull/17" "https://github.com/acme/release-console/commit/deadbeef"))"#
    ]];
    ParityBatchCase::value(
        "detects_a_real_git_remote_and_formats_repository_links",
        elisp_form,
        expected,
    )
}

fn imports_and_reloads_repository_topics_from_the_real_database() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((workspace (neomacs-forge-test-open-workspace t))
       (root (plist-get workspace :root))
       (repo (plist-get workspace :repo))
       (default-directory (file-name-as-directory root)))
  (unwind-protect
      (let* ((seed (neomacs-forge-test-seed-repository repo))
             (repo-id (oref repo id))
             (before
              (list :tables
                    (list (forge-sql1 [:select (funcall count *) :from repository])
                          (forge-sql1 [:select (funcall count *) :from issue])
                          (forge-sql1 [:select (funcall count *) :from issue-post])
                          (forge-sql1 [:select (funcall count *) :from pullreq])
                          (forge-sql1 [:select (funcall count *) :from pullreq-post])
                          (forge-sql1 [:select (funcall count *) :from label])
                          (forge-sql1 [:select (funcall count *) :from assignee]))
                    :issue-id (oref (plist-get seed :issue) id)
                    :pullreq-id (oref (plist-get seed :pullreq) id))))
        (neomacs-forge-test-close-database)
        (let* ((reloaded-repo (forge-get-repository :id repo-id))
               (issue (forge-get-issue reloaded-repo 42))
               (pullreq (forge-get-pullreq reloaded-repo 17)))
          (list
           :before before
           :repo
           (list (oref reloaded-repo condition)
                 (oref reloaded-repo description)
                 (oref reloaded-repo default-branch)
                 (oref reloaded-repo private-p)
                 (oref reloaded-repo stars)
                 (oref reloaded-repo watchers))
           :issue
           (list (oref issue number) (oref issue state) (oref issue status)
                 (oref issue author) (oref issue title) (oref issue body)
                 (mapcar #'cadr (oref issue labels))
                 (mapcar #'cadr (oref issue assignees))
                 (mapcar (lambda (post)
                           (list (oref post number) (oref post author)
                                 (oref post body)))
                         (oref issue posts)))
           :pullreq
           (list (oref pullreq number) (oref pullreq state)
                 (oref pullreq status) (oref pullreq draft-p)
                 (oref pullreq cross-repo-p)
                 (oref pullreq base-ref) (oref pullreq head-ref)
                 (oref pullreq head-user) (oref pullreq head-repo)
                 (mapcar #'cadr (oref pullreq labels))
                 (mapcar #'cadr (oref pullreq review-requests))
                 (mapcar (lambda (post)
                           (list (oref post number) (oref post author)
                                 (oref post body)))
                         (oref pullreq posts))))))
    (neomacs-forge-test-clean-workspace workspace)))
"####;
    let expected = expect![[
        r#"OK (:before (:tables (1 1 1 1 1 2 2) :issue-id "Z2l0aHViLmNvbTphY21lL3JlbGVhc2UtY29uc29sZTppc3N1ZTQy" :pullreq-id "Z2l0aHViLmNvbTphY21lL3JlbGVhc2UtY29uc29sZTpwdWxscmVxMTc=") :repo (:tracked "Release orchestration Ω" "master" t 128 17) :issue (42 open unread "alice" "Deployment fails for 東京 region" "Reproduce with `deploy --region 東京`.\nSecond line." ("bug" "release") ("alice") ((4201 "bob" "Confirmed on canary Ω."))) :pullreq (17 open pending t t "master" "retry-region" "bob" "contributors/release-console" ("release") ("bob") ((1701 "alice" "Canary is green; ready for review."))))"#
    ]];
    ParityBatchCase::value(
        "imports_and_reloads_repository_topics_from_the_real_database",
        elisp_form,
        expected,
    )
}

fn renders_and_refilters_the_active_topic_list_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((workspace (neomacs-forge-test-open-workspace t))
       (root (plist-get workspace :root))
       (repo (plist-get workspace :repo))
       (default-directory (file-name-as-directory root)))
  (unwind-protect
      (progn
        (neomacs-forge-test-seed-repository repo)
        (forge-topics-setup-buffer
         repo (forge--topics-spec :type 'topic :active t
                                  :order 'newest :limit nil))
        (let* ((buffer (get-buffer (forge-topics-buffer-name repo)))
               (active
                (with-current-buffer buffer
                  (list :mode major-mode
                        :description (forge-topics-buffer-desc)
                        :text (string-trim-right
                               (buffer-substring-no-properties
                                (point-min) (point-max)))
                        :topics
                        (mapcar (lambda (topic)
                                  (list (eieio-object-class-name topic)
                                        (oref topic number)
                                        (oref topic status)))
                                (forge--list-topics
                                 forge--buffer-topics-spec repo))))))
          (with-current-buffer buffer
            (setq forge--buffer-topics-spec
                  (forge--topics-spec :type 'issue :active nil :state nil
                                      :labels '("bug") :order 'oldest
                                      :limit nil))
            (let ((inhibit-read-only t))
              (erase-buffer)
              (forge-topics-refresh-buffer)))
          (list
           :active active
           :bug-issues
           (with-current-buffer buffer
             (list :description (forge-topics-buffer-desc)
                   :text (string-trim-right
                          (buffer-substring-no-properties
                           (point-min) (point-max)))
                   :topics
                   (mapcar (lambda (topic)
                             (list (oref topic number) (oref topic title)))
                           (forge--list-topics
                            forge--buffer-topics-spec repo)))))))
    (neomacs-forge-test-clean-workspace workspace)))
"####;
    let expected = expect![[
        r##"OK (:active (:mode forge-topics-mode :description "Topics" :text "#42   Deployment fails for 東京 region bug release\n#17   Retry regional deployment atomically release" :topics ((forge-issue 42 unread) (forge-pullreq 17 pending))) :bug-issues (:description "Issues" :text "#42   Deployment fails for 東京 region bug release" :topics ((42 "Deployment fails for 東京 region"))))"##
    ]];
    ParityBatchCase::value(
        "renders_and_refilters_the_active_topic_list_buffer",
        elisp_form,
        expected,
    )
}

fn reads_committed_issue_and_pull_request_templates() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((workspace (neomacs-forge-test-open-workspace t))
       (root (plist-get workspace :root))
       (repo (plist-get workspace :repo))
       (default-directory (file-name-as-directory root))
       (issue-dir (expand-file-name ".github/ISSUE_TEMPLATE" root)))
  (unwind-protect
      (progn
        (neomacs-forge-test-seed-repository repo)
        (make-directory issue-dir t)
        (with-temp-file (expand-file-name "bug.md" issue-dir)
          (insert "---\nname: Regional incident\nabout: Report a deployment failure\n"
                  "title: 'incident: '\nlabels: [bug, unknown]\n"
                  "assignees: [alice, ghost]\n---\n"
                  "## Environment\n\nRegion: 東京\n"))
        (with-temp-file (expand-file-name "config.yml" issue-dir)
          (insert "blank_issues_enabled: false\ncontact_links:\n"
                  "  - name: Release support\n"
                  "    url: https://support.example.test/releases\n"
                  "    about: Escalate a production incident\n"))
        (with-temp-file (expand-file-name "PULL_REQUEST_TEMPLATE.md" root)
          (insert "## Rollout plan\n\n- [ ] Canary\n- [ ] Production Ω\n"))
        (neomacs-forge-test-git root "add" ".github" "PULL_REQUEST_TEMPLATE.md")
        (neomacs-forge-test-git root "commit" "-q" "-m" "Add contribution templates")
        (neomacs-forge-test-git
         root "update-ref" "refs/remotes/origin/master" "HEAD")
        (neomacs-forge-test-plain
         (list
          :issue (forge--topic-templates repo 'forge-issue)
          :pullreq (forge--topic-templates repo 'forge-pullreq))))
    (neomacs-forge-test-clean-workspace workspace)))
"####;
    let expected = expect![[
        r###"OK (:issue (((prompt . "Regional incident — Report a deployment failure") (title . "incident:") (text . "## Environment\n\nRegion: 東京") (labels "bug") (assignees "alice") (draft)) ((url . "https://support.example.test/releases") (about . "Escalate a production incident") (name . "Release support") (prompt . "Release support — Escalate a production incident"))) :pullreq (((prompt . "master:PULL_REQUEST_TEMPLATE") (text . "## Rollout plan\n\n- [ ] Canary\n- [ ] Production Ω"))))"###
    ]];
    ParityBatchCase::value(
        "reads_committed_issue_and_pull_request_templates",
        elisp_form,
        expected,
    )
}

fn browses_topics_and_resolves_references_at_point() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((workspace (neomacs-forge-test-open-workspace t))
       (root (plist-get workspace :root))
       (repo (plist-get workspace :repo))
       (default-directory (file-name-as-directory root))
       (seed (neomacs-forge-test-seed-repository repo))
       (issue (plist-get seed :issue))
       (pullreq (plist-get seed :pullreq))
       visited)
  (unwind-protect
      (let ((references
             (with-temp-buffer
               (setq default-directory (file-name-as-directory root)
                     forge-buffer-repository (oref repo id))
               (insert "Release blocked by #42; replacement is #17.")
               (goto-char (point-min))
               (search-forward "#42")
               (let ((found-issue (forge-thingatpt--issue)))
                 (search-forward "#17")
                 (let ((found-pullreq (forge-thingatpt--pullreq)))
                   (list (and found-issue (oref found-issue number))
                         (and found-pullreq (oref found-pullreq number))))))))
        (cl-letf (((symbol-function 'browse-url)
                   (lambda (url &rest _) (push url visited))))
          (forge-browse-issue issue)
          (forge-browse-pullreq pullreq)
          (forge-browse-repository repo)
          (forge-browse-blob "master" "src/release.rs" 10 14 nil))
        (list :references references
              :visited (nreverse visited)
              :issue-status (oref issue status)
              :pullreq-status (oref pullreq status)))
    (neomacs-forge-test-clean-workspace workspace)))
"####;
    let expected = expect![[
        r#"OK (:references (42 17) :visited ("https://github.com/acme/release-console/issues/42" "https://github.com/acme/release-console/pull/17" "https://github.com/acme/release-console" "https://github.com/acme/release-console/blob/master/src/release.rs#L10-L14") :issue-status pending :pullreq-status pending)"#
    ]];
    ParityBatchCase::value(
        "browses_topics_and_resolves_references_at_point",
        elisp_form,
        expected,
    )
}

fn persists_personal_marks_saved_state_and_read_state() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((workspace (neomacs-forge-test-open-workspace t))
       (root (plist-get workspace :root))
       (repo (plist-get workspace :repo))
       (default-directory (file-name-as-directory root))
       (issue (plist-get (neomacs-forge-test-seed-repository repo) :issue)))
  (unwind-protect
      (progn
        (forge-sql [:insert-into mark :values $v1]
                   [nil "mark-release" "release-blocker" error
                    "Blocks the next release"])
        (forge-sql [:insert-into mark :values $v1]
                   [nil "mark-triage" "triage" warning
                    "Needs an owner"])
        (cl-letf (((symbol-function 'forge-refresh-buffer) #'ignore))
          (forge--set-topic-marks repo issue '("triage" "release-blocker"))
          (forge-topic-mark-read issue)
          (with-temp-buffer
            (setq forge-buffer-topic issue)
            (forge-topic-toggle-saved)))
        ;; Real Forge buffers reload their Closql objects while refreshing.
        ;; Do the same after stubbing only that UI refresh above, so derived
        ;; relation slots contain rows rather than the just-written ids.
        (setq issue (closql-reload issue))
        (let ((formatted
               (neomacs-forge-test-plain
                (list (forge--format-topic-line issue 5)
                      (forge--format-marks issue t)
                      (forge--format-topic-state issue)
                      (forge--format-topic-status issue)))))
          (neomacs-forge-test-close-database)
          (let ((reloaded (forge-get-issue
                           (forge-get-repository :id (oref repo id)) 42)))
            (list :formatted formatted
                  :persisted
                  (list (oref reloaded status)
                        (oref reloaded saved-p)
                        (mapcar #'cadr (oref reloaded marks)))))))
    (neomacs-forge-test-clean-workspace workspace)))
"####;
    let expected = expect![[
        r##"OK (:formatted ("#42   Deployment fails for 東京 region" "release-blocker triage" "open" "pending") :persisted (pending t ("release-blocker" "triage")))"##
    ]];
    ParityBatchCase::value(
        "persists_personal_marks_saved_state_and_read_state",
        elisp_form,
        expected,
    )
}

fn adds_the_pull_request_refspec_once_and_fetches_once() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((workspace (neomacs-forge-test-open-workspace t))
       (root (plist-get workspace :root))
       (default-directory (file-name-as-directory root))
       fetches)
  (unwind-protect
      (cl-letf (((symbol-function 'magit-git-fetch)
                 (lambda (&rest args) (push args fetches) 'queued)))
        (forge-add-pullreq-refspec)
        (let ((after-first
               (list (magit-get-all "remote" "origin" "fetch")
                     (forge--pullreq-refspec)
                     (length fetches))))
          (forge-add-pullreq-refspec)
          (list :after-first after-first
                :after-second
                (list (magit-get-all "remote" "origin" "fetch")
                      (forge--pullreq-refspec)
                      (length fetches)
                      (current-message))
                :fetches (nreverse fetches))))
    (neomacs-forge-test-clean-workspace workspace)))
"####;
    let expected = expect![[
        r#"OK (:after-first (("+refs/heads/*:refs/remotes/origin/*" "+refs/pull/*/head:refs/pullreqs/*") "+refs/pull/*/head:refs/pullreqs/*" 1) :after-second (("+refs/heads/*:refs/remotes/origin/*" "+refs/pull/*/head:refs/pullreqs/*") "+refs/pull/*/head:refs/pullreqs/*" 1 nil) :fetches (("origin" nil)))"#
    ]];
    ParityBatchCase::value(
        "adds_the_pull_request_refspec_once_and_fetches_once",
        elisp_form,
        expected,
    )
}

fn forge_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FORGE_MELPA_PIN, "forge.el")
        .expect("prepare pinned Forge and exact dependencies below ./tmp")
        .with_timeout(Duration::from_secs(600))
        .with_prelude(PRELUDE)
}

#[test]
fn forge_practical_workflows_batch() {
    let cases = vec![
        detects_a_real_git_remote_and_formats_repository_links(),
        imports_and_reloads_repository_topics_from_the_real_database(),
        renders_and_refilters_the_active_topic_list_buffer(),
        reads_committed_issue_and_pull_request_templates(),
        browses_topics_and_resolves_references_at_point(),
        persists_personal_marks_saved_state_and_read_state(),
        adds_the_pull_request_refspec_once_and_fetches_once(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("Forge practical workflow parity batch");
    assert_oracle_batch_cases(forge_oracle(), test_name, "forge parity", &cases);
}
