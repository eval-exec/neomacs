use expect_test::expect;

use super::ParityBatchCase;

fn agent_recall_reindexes_real_project_and_org_transcripts_then_reloads_the_persisted_index()
-> ParityBatchCase {
    ParityBatchCase::value(
        "agent_recall_reindexes_real_project_and_org_transcripts_then_reloads_the_persisted_index",
        r####"(let* ((root
                                 (expand-file-name
                                  "library"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (projects
                                 (expand-file-name
                                  "projects"
                                  root))
                                (alpha-project
                                 (expand-file-name
                                  "alpha"
                                  projects))
                                (alpha-dir
                                 (expand-file-name
                                  ".agent-shell/transcripts"
                                  alpha-project))
                                (beta-dir
                                 (expand-file-name
                                  "nested/beta/.agent-shell/transcripts"
                                  projects))
                                (org-dir
                                 (expand-file-name
                                  "org-transcripts"
                                  root))
                                (alpha-file
                                 (expand-file-name
                                  "2026-07-10-17-07-00.md"
                                  alpha-dir))
                                (beta-file
                                 (expand-file-name
                                  "2026-07-11-09-30-00.md"
                                  beta-dir))
                                (org-file
                                 (expand-file-name
                                  "planning-session.org"
                                  org-dir))
                                (agent-recall-search-paths
                                 (list projects))
                                (agent-recall-extra-transcript-dirs
                                 (list
                                  (list
                                   :dir
                                   org-dir
                                   :project
                                   "research-notes")))
                                (agent-recall-index-file
                                 (expand-file-name
                                  "state/index.el"
                                  root))
                                (agent-recall-browse-sort
                                 'date-desc)
                                (agent-recall--index nil)
                                (agent-recall--index-loaded-p nil)
                                (agent-recall--session-id-cache
                                 (make-hash-table
                                  :test
                                  'equal)))
                           (make-directory
                            alpha-dir
                            t)
                           (make-directory
                            beta-dir
                            t)
                           (make-directory
                            org-dir
                            t)
                           (with-temp-file alpha-file
                             (insert
                              "**Started:** 2026-07-10 17:07:00\n"
                              "**Agent:** Claude Code\n"
                              "**Working Directory:** "
                              alpha-project
                              "\n"
                              "**Session:** aaaaaaaa-1111-2222-3333-444444444444\n\n"
                              "---\n\n"
                              "## User\n"
                              "> Repair the production parser without changing its public API.\n\n"
                              "## Agent\n"
                              "I will trace the parser state transitions.\n"))
                           (with-temp-file beta-file
                             (insert
                              "**Started:** 2026-07-11 09:30:00\n"
                              "**Agent:** Claude Code\n"
                              "**Working Directory:** "
                              (expand-file-name
                               "nested/beta"
                               projects)
                              "\n\n"
                              "---\n\n"
                              "## User\n"
                              "> Explain why the deployment cache is stale.\n\n"
                              "## Agent\n"
                              "The cache key omits the target triple.\n"))
                           (with-temp-file org-file
                             (insert
                              "#+TITLE: Research session\n"
                              "#+DATE: 2026-07-09 14:15:00\n"
                              "#+PROPERTY: Working_Directory /work/research\n"
                              "#+PROPERTY: Agent Claude Code\n"
                              "#+PROPERTY: Session bbbbbbbb-1111-2222-3333-444444444444\n\n"
                              "** User\n"
                              "#+begin_quote\n"
                              "Compare the two garbage collection strategies.\n"
                              "#+end_quote\n\n"
                              "** Assistant\n"
                              "The tracing strategy preserves cycles.\n"))
                           (with-temp-file
                               (expand-file-name
                                "notes.txt"
                                alpha-dir)
                             (insert
                              "not a transcript"))
                           (unwind-protect
                               (progn
                                 (agent-recall-reindex)
                                 (let* ((reindex-message
                                         (current-message))
                                        (summarize
                                         (lambda ()
                                           (mapcar
                                            (lambda (file)
                                              (let ((entry
                                                     (gethash
                                                      file
                                                      agent-recall--index)))
                                                (list
                                                 (file-relative-name
                                                  file
                                                  root)
                                                 (plist-get
                                                  entry
                                                  :project)
                                                 (plist-get
                                                  entry
                                                  :timestamp)
                                                 (plist-get
                                                  entry
                                                  :session-id)
                                                 (plist-get
                                                  entry
                                                  :preview))))
                                            (sort
                                             (hash-table-keys
                                              agent-recall--index)
                                             #'string<))))
                                        (initial
                                         (funcall summarize))
                                        (browse
                                         (mapcar
                                          (lambda (entry)
                                            (cons
                                             (car entry)
                                             (file-relative-name
                                              (cdr entry)
                                              root)))
                                          (agent-recall--list-transcripts))))
                                   (agent-recall-invalidate-cache)
                                   (agent-recall--index-ensure)
                                   (let ((reloaded
                                          (funcall summarize)))
                                     (agent-recall-stats)
                                     (list
                                      reindex-message
                                      (file-exists-p
                                       agent-recall-index-file)
                                      initial
                                      browse
                                      reloaded
                                      (with-current-buffer
                                          "*agent-recall-stats*"
                                        (buffer-substring-no-properties
                                         (point-min)
                                         (point-max)))
                                      (gethash
                                       (expand-file-name
                                        "notes.txt"
                                        alpha-dir)
                                       agent-recall--index)))))
                             (when-let ((buffer
                                         (get-buffer
                                          "*agent-recall-stats*")))
                               (kill-buffer buffer))
                             (delete-directory
                              root
                              t)))"####,
        expect![[
            r#"OK (nil t (("org-transcripts/planning-session.org" "research-notes" "planning-session" "bbbbbbbb-1111-2222-3333-444444444444" "Compare the two garbage collection strategies.") ("projects/alpha/.agent-shell/transcripts/2026-07-10-17-07-00.md" "alpha" "2026-07-10-17-07-00" "aaaaaaaa-1111-2222-3333-444444444444" "Repair the production parser without changing its public API.") ("projects/nested/beta/.agent-shell/transcripts/2026-07-11-09-30-00.md" "beta" "2026-07-11-09-30-00" nil "Explain why the deployment cache is stale.")) (("[research-notes] planning-session" . "org-transcripts/planning-session.org") ("[beta] 2026-07-11-09-30-00" . "projects/nested/beta/.agent-shell/transcripts/2026-07-11-09-30-00.md") ("[alpha] 2026-07-10-17-07-00" . "projects/alpha/.agent-shell/transcripts/2026-07-10-17-07-00.md")) (("org-transcripts/planning-session.org" "research-notes" "planning-session" "bbbbbbbb-1111-2222-3333-444444444444" "Compare the two garbage collection strategies.") ("projects/alpha/.agent-shell/transcripts/2026-07-10-17-07-00.md" "alpha" "2026-07-10-17-07-00" "aaaaaaaa-1111-2222-3333-444444444444" "Repair the production parser without changing its public API.") ("projects/nested/beta/.agent-shell/transcripts/2026-07-11-09-30-00.md" "beta" "2026-07-11-09-30-00" nil "Explain why the deployment cache is stale.")) "Agent Recall -- Transcript Statistics\n════════════════════════════════════════\n\n  Transcripts: 3\n  Projects:    3\n  Total size:  0.0 MB\n\nBy Project:\n────────────────────────────────────────\n  research-notes                    1 files  (0.0 MB)\n  beta                              1 files  (0.0 MB)\n  alpha                             1 files  (0.0 MB)\n" nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![agent_recall_reindexes_real_project_and_org_transcripts_then_reloads_the_persisted_index()]
}
