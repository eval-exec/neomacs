use expect_test::expect;

use super::ParityBatchCase;

/// The block the Commentary documents for installation:
///
///     (use-package agitjo
///       :config (agitjo-setup "#"))
///
/// `agitjo-setup' has to do two separate things for that one line to work --
/// bind the key in `magit-status-mode-map', which is step 2 of the documented
/// workflow ("or by inputting the `#' key inside a Magit status buffer"), and
/// append the same key to `magit-dispatch' so it is reachable from Magit's
/// global menu.  Both are asserted before and after, against a Magit dispatch
/// menu that has to be otherwise untouched: exactly one key is added, and no
/// existing entry moves.
///
/// The `agitjo-push' menu's own layout is pinned whole beside it, because that
/// is the surface steps 3 and 4 of the documented workflow are performed on.
fn the_documented_setup_block_adds_one_key_to_magit_status_and_magit_dispatch() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_documented_setup_block_adds_one_key_to_magit_status_and_magit_dispatch",
        r##"(let* ((before (agitjo-test-transient-keys 'magit-dispatch))
       (observed nil))
  (push (list :before
              (list :status-key (keymap-lookup magit-status-mode-map "#")
                    :dispatch-entry (copy-tree (assoc "#" before))
                    :dispatch-keys (length before)))
        observed)
  (agitjo-setup "#")
  (let* ((after (agitjo-test-transient-keys 'magit-dispatch))
         (added (seq-remove (lambda (entry) (member entry before)) after)))
    (push (list :after
                (list :status-key (keymap-lookup magit-status-mode-map "#")
                      :dispatch-entry (copy-tree (assoc "#" after))
                      :dispatch-keys (length after)
                      :added (copy-tree added)
                      :nothing-else-changed
                      (equal (seq-remove (lambda (entry) (member entry added))
                                         after)
                             before)))
          observed))
  (push (list :agitjo-push-menu (agitjo-test-transient-keys 'agitjo-push))
        observed)
  (nreverse observed))"##,
        expect![[
            r##"OK ((:before (:status-key nil :dispatch-entry nil :dispatch-keys 51)) (:after (:status-key agitjo-push :dispatch-entry ("#" . agitjo-push) :dispatch-keys 52 :added (("#" . agitjo-push)) :nothing-else-changed t)) (:agitjo-push-menu (("-f" . agitjo-force-push-switch) ("-s" . agitjo-topic-variable) ("-t" . agitjo-title-option) ("+" . agitjo--pullreq-type-switches) ("u" . agitjo-push-pullreq-current-to-upstream) ("e" . agitjo-push-pullreq-current) ("l" . agitjo-push-pullreq-local-branch) ("r" . agitjo-push-pullreq-local-branch-or-ref) ("C" . magit-branch-configure) ("V" . agitjo-visit-last-pushed-pullreq))))"##
        ]],
    )
}

fn without_a_session_topic_the_refspec_falls_back_to_each_projects_source_branch() -> ParityBatchCase
{
    ParityBatchCase::value(
        "without_a_session_topic_the_refspec_falls_back_to_each_projects_source_branch",
        r##"(let* ((agitjo--current-topics nil)
       (agitjo-test-push-requests nil)
       (agitjo-test-sentinel-events nil)
       (observed nil))
  (dolist (case (list (list :untopiced "parser-repo" "feature/parser-recovery" nil)
                      (list :topiced "docs-repo" "feature/handbook" "team/handbook-42")))
    (let* ((root (agitjo-test-repo (nth 1 case)
                                   '(("README.md" . "# Project\n"))))
           (default-directory root)
           (branch (agitjo-test-branch root (nth 2 case)
                                       '(("src/change.el" . "(provide 'change)\n"))
                                       "Change the thing"))
           (config nil))
      (when (nth 3 case) (agitjo--set-current-topic (nth 3 case)))
      (setq config (agitjo--pullreq-configuration
                    :type "for" :source branch :target "origin/main"
                    :args '("normal")))
      (let ((buffer (agitjo-post--buffer)))
        (set-window-buffer (selected-window) buffer)
        (set-buffer buffer)
        (agitjo-post-mode)
        (setq-local agitjo-post--pullreq-config config)
        (erase-buffer)
        (insert "Describe the change.\n")
        (cl-letf (((symbol-function 'magit-run-git-async)
                   (agitjo-test-push-stand-in 0))
                  ((symbol-function 'magit-process-sentinel)
                   #'agitjo-test-record-sentinel))
          (execute-kbd-macro (kbd "C-c C-c"))
          (agitjo-test-await agitjo-test-last-process)))
      (push (list (car case)
                  (list :topic (agitjo--get-current-topic)
                        :refspec (nth 3 (car (last (agitjo-test-requests))))))
            observed)))
  (nreverse observed))"##,
        expect![[
            r#"OK ((:untopiced (:topic nil :refspec "feature/parser-recovery:refs/for/main/feature/parser-recovery")) (:topiced (:topic "team/handbook-42" :refspec "feature/handbook:refs/for/main/team/handbook-42")))"#
        ]],
    )
}

fn the_draft_switch_titles_the_request_wip_from_the_commit_subject_when_none_is_given()
-> ParityBatchCase {
    ParityBatchCase::value(
        "the_draft_switch_titles_the_request_wip_from_the_commit_subject_when_none_is_given",
        r##"(let* ((agitjo--current-topics nil)
       (agitjo-test-push-requests nil)
       (agitjo-test-sentinel-events nil)
       (root (agitjo-test-repo "titles-repo" '(("README.md" . "# Project\n"))))
       (default-directory root)
       (branch (agitjo-test-branch
                root "feature/lookahead"
                '(("src/parser.el" . "(defun parser-state () 'recovered)\n"))
                "Recover parser transitions after lookahead reset"))
       (observed nil))
  (dolist (case (list (list :draft-with-an-explicit-title
                            '("draft" "--push-option=title=Parser recovery"))
                      (list :draft-without-a-title '("draft"))
                      (list :normal-with-an-explicit-title
                            '("normal" "--push-option=title=Parser recovery"))))
    (let ((config (agitjo--pullreq-configuration
                   :type "for" :source branch :target "origin/main"
                   :args (copy-sequence (nth 1 case))))
          (buffer (agitjo-post--buffer)))
      (set-window-buffer (selected-window) buffer)
      (set-buffer buffer)
      (agitjo-post-mode)
      (setq-local agitjo-post--pullreq-config config)
      (erase-buffer)
      (insert "Reset lookahead after recovery.\n")
      (cl-letf (((symbol-function 'magit-run-git-async)
                 (agitjo-test-push-stand-in 0))
                ((symbol-function 'magit-process-sentinel)
                 #'agitjo-test-record-sentinel))
        (execute-kbd-macro (kbd "C-c C-c"))
        (agitjo-test-await agitjo-test-last-process))
      (push (list (car case)
                  (seq-filter (lambda (argument)
                                (and (stringp argument)
                                     (string-prefix-p "--push-option=title="
                                                      argument)))
                              (car (last (agitjo-test-requests)))))
            observed)))
  (push (list :commit-subject (magit-rev-format "%s" branch)) observed)
  (nreverse observed))"##,
        expect![[
            r#"OK ((:draft-with-an-explicit-title ("--push-option=title=WIP: Parser recovery")) (:draft-without-a-title ("--push-option=title=WIP: Recover parser transitions after lookahead reset")) (:normal-with-an-explicit-title ("--push-option=title=Parser recovery")) (:commit-subject "Recover parser transitions after lookahead reset"))"#
        ]],
    )
}

fn visiting_the_last_pushed_pull_request_takes_the_most_recent_link_from_git_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "visiting_the_last_pushed_pull_request_takes_the_most_recent_link_from_git_output",
        r##"(let* ((agitjo--current-topics nil)
       (agitjo-test-push-requests nil)
       (agitjo-test-sentinel-events nil)
       (root (agitjo-test-repo "visit-repo" '(("README.md" . "# Project\n"))))
       (default-directory root)
       (branch (agitjo-test-branch root "feature/visit"
                                   '(("src/visit.el" . "(provide 'visit)\n"))
                                   "Add the visit path"))
       (visited nil)
       (observed nil))
  (cl-letf (((symbol-function 'browse-url)
             (lambda (url &rest _) (push (copy-sequence url) visited) 'opened)))
    (push (list :before-any-push
                (condition-case error (agitjo-visit-last-pushed-pullreq)
                  (error (list (car error) (cadr error))))
                :opened (reverse visited))
          observed)
    (dolist (push-case (list (list 41 "feature/visit")
                             (list 42 "feature/visit")))
      (let ((config (agitjo--pullreq-configuration
                     :type "for" :source branch :target "origin/main"
                     :args '("normal")))
            (buffer (agitjo-post--buffer)))
        (set-window-buffer (selected-window) buffer)
        (set-buffer buffer)
        (agitjo-post-mode)
        (setq-local agitjo-post--pullreq-config config)
        (erase-buffer)
        (insert "Describe the change.\n")
        (cl-letf (((symbol-function 'magit-run-git-async)
                   (agitjo-test-push-stand-in
                    0
                    (format (concat "remote: Resolving deltas: 100%% (1/1)\n"
                                    "remote: Repository at https://forge.invalid/halvin/agitjo\n"
                                    "remote: See https://forge.invalid/halvin/agitjo/pulls/%d for details\n"
                                    "remote:   https://forge.invalid/halvin/agitjo/pulls/%d\n"
                                    "To ssh://forge.invalid/halvin/agitjo.git\n"
                                    " * [new reference] %s -> refs/for/main/%s\n")
                            (- (car push-case) 10) (car push-case)
                            (nth 1 push-case) (nth 1 push-case))))
                  ((symbol-function 'magit-process-sentinel)
                   #'agitjo-test-record-sentinel))
          (execute-kbd-macro (kbd "C-c C-c"))
          (agitjo-test-await agitjo-test-last-process))))
    (push (list :after-two-pushes
                (agitjo-visit-last-pushed-pullreq)
                :opened (reverse visited))
          observed))
  (nreverse observed))"##,
        expect![[
            r#"OK ((:before-any-push (user-error "No pull request link could be found") :opened nil) (:after-two-pushes opened :opened ("https://forge.invalid/halvin/agitjo/pulls/42")))"#
        ]],
    )
}

fn the_draft_template_comes_from_the_remote_main_branch_and_prefers_forgejo() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_draft_template_comes_from_the_remote_main_branch_and_prefers_forgejo",
        r####"(let* ((agitjo--current-topics nil)
       (observed nil))
  ;; A template on origin/main, a different one committed on the feature
  ;; branch, and a third one only in the working tree.
  (let* ((root (agitjo-test-repo
                "template-origin"
                '((".github/pull_request_template.md"
                   . "## From origin main\n\nDescribe the change.\n"))))
         (default-directory root))
    (agitjo-test-branch root "feature/templates"
                        '((".forgejo/PULL_REQUEST_TEMPLATE.md"
                           . "## From the feature branch\n")
                          ("src/thing.el" . "(provide 'thing)\n"))
                        "Add the thing")
    (agitjo-test-write (expand-file-name ".gitea/PULL_REQUEST_TEMPLATE.md" root)
                       "## Only in the working tree\n")
    (let ((config (agitjo--pullreq-configuration
                   :type "for" :source "feature/templates" :target "origin/main"
                   :args '("normal")))
          (buffer nil))
      (agitjo-post--setup-buffer config)
      (setq buffer (agitjo-post--buffer))
      (push (list :three-candidate-templates
                  (list :draft (with-current-buffer buffer (buffer-string))
                        :draft-file (agitjo-test-relative
                                     (buffer-file-name buffer))))
            observed)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  ;; Both a .forgejo and a .github template committed on main.
  (let* ((root (agitjo-test-repo
                "template-precedence"
                '((".forgejo/PULL_REQUEST_TEMPLATE.md" . "## Forgejo template\n")
                  (".github/pull_request_template.md" . "## GitHub template\n"))))
         (default-directory root))
    (agitjo-test-branch root "feature/precedence"
                        '(("src/thing.el" . "(provide 'thing)\n"))
                        "Add the thing")
    (let ((config (agitjo--pullreq-configuration
                   :type "for" :source "feature/precedence" :target "origin/main"
                   :args '("normal")))
          (buffer nil))
      (agitjo-post--setup-buffer config)
      (setq buffer (agitjo-post--buffer))
      (push (list :forgejo-and-github-templates
                  (list :draft (with-current-buffer buffer (buffer-string))))
            observed)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (nreverse observed))"####,
        expect![[
            r###"OK ((:three-candidate-templates (:draft "## From origin main\n\nDescribe the change.\n" :draft-file "template-origin/.git/agitjo/pullreq-draft")) (:forgejo-and-github-templates (:draft "## Forgejo template\n")))"###
        ]],
    )
}

fn confirming_from_an_unrelated_buffer_refuses_and_pushes_nothing() -> ParityBatchCase {
    ParityBatchCase::value(
        "confirming_from_an_unrelated_buffer_refuses_and_pushes_nothing",
        r##"(let* ((agitjo--current-topics nil)
       (agitjo-test-push-requests nil)
       (root (agitjo-test-repo "guard-repo" '(("README.md" . "# Project\n"))))
       (default-directory root)
       (draft-file (expand-file-name ".git/agitjo/pullreq-draft" root))
       (scratch (get-buffer-create "*agitjo-unrelated*"))
       (observed nil))
  (agitjo-test-branch root "feature/guard"
                      '(("src/guard.el" . "(provide 'guard)\n"))
                      "Add the guard")
  (push (list :before (list :draft-directory-exists
                            (file-directory-p (file-name-directory draft-file))
                            :draft (agitjo-test-draft-contents draft-file)))
        observed)
  (with-current-buffer scratch
    (setq default-directory root)
    (insert "not a pull request draft\n")
    (set-window-buffer (selected-window) scratch)
    (push (list :refusal
                (condition-case error (call-interactively #'agitjo-post-confirm)
                  (error (list (car error) (cadr error)))))
          observed))
  (push (list :after (list :draft-directory-exists
                           (file-directory-p (file-name-directory draft-file))
                           :draft (agitjo-test-draft-contents draft-file)
                           :pushes (agitjo-test-requests)
                           :unrelated-buffer-text
                           (with-current-buffer scratch (buffer-string))))
        observed)
  (nreverse observed))"##,
        expect![[
            r#"OK ((:before (:draft-directory-exists nil :draft no-draft-file)) (:refusal (user-error "Function called outside AGitjo post buffer")) (:after (:draft-directory-exists t :draft no-draft-file :pushes nil :unrelated-buffer-text "not a pull request draft\n")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_documented_setup_block_adds_one_key_to_magit_status_and_magit_dispatch(),
        without_a_session_topic_the_refspec_falls_back_to_each_projects_source_branch(),
        the_draft_switch_titles_the_request_wip_from_the_commit_subject_when_none_is_given(),
        visiting_the_last_pushed_pull_request_takes_the_most_recent_link_from_git_output(),
        the_draft_template_comes_from_the_remote_main_branch_and_prefers_forgejo(),
        confirming_from_an_unrelated_buffer_refuses_and_pushes_nothing(),
    ]
}
