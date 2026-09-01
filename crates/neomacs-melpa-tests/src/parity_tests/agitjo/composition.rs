use expect_test::expect;

use super::ParityBatchCase;

fn agitjo_composes_edits_and_cancels_a_real_template_backed_pull_request_draft() -> ParityBatchCase
{
    ParityBatchCase::value(
        "agitjo_composes_edits_and_cancels_a_real_template_backed_pull_request_draft",
        r####"(let* ((root
                                  (file-name-as-directory
                                   (expand-file-name
                                    "agitjo-composition"
                                    (getenv
                                     "NEOMACS_TEST_SANDBOX_ROOT"))))
                                 (default-directory
                                  root)
                                 (git
                                  (lambda (&rest arguments)
                                    (with-temp-buffer
                                      (let ((status
                                             (apply
                                              #'process-file
                                              "git"
                                              nil
                                              t
                                              nil
                                              arguments)))
                                        (unless
                                            (zerop status)
                                          (error
                                           "git %S failed: %s"
                                           arguments
                                           (buffer-string)))
                                        (string-trim
                                         (buffer-string))))))
                                 (template
                                  (expand-file-name
                                   ".forgejo/PULL_REQUEST_TEMPLATE.md"
                                   root))
                                 (source
                                  (expand-file-name
                                   "src/parser.el"
                                   root))
                                 (agitjo--current-topics
                                  nil)
                                 post-buffer
                                 draft-file
                                 result)
                             (unwind-protect
                                 (progn
                                   (when
                                       (file-exists-p
                                        root)
                                     (delete-directory
                                      root
                                      t))
                                   (make-directory
                                    (file-name-directory
                                     template)
                                    t)
                                   (make-directory
                                    (file-name-directory
                                     source)
                                    t)
                                   (funcall
                                    git
                                    "init"
                                    "-b"
                                    "main")
                                   (funcall
                                    git
                                    "config"
                                    "user.name"
                                    "Neomacs Oracle")
                                   (funcall
                                    git
                                    "config"
                                    "user.email"
                                    "oracle@example.invalid")
                                   (with-temp-file
                                       template
                                     (insert
                                      "---\n"
                                      "name: Pull request\n"
                                      "about: Describe the change\n"
                                      "---\n"
                                      "## Summary\n\n"
                                      "Explain the user-visible effect.\n\n"
                                      "## Checklist\n\n"
                                      "- [ ] Tests pass\n"))
                                   (with-temp-file
                                       source
                                     (insert
                                      "(defun parser-state () 'old)\n"))
                                   (funcall
                                    git
                                    "add"
                                    ".")
                                   (funcall
                                    git
                                    "commit"
                                    "-m"
                                    "Establish parser baseline")
                                   (let ((main-commit
                                          (funcall
                                           git
                                           "rev-parse"
                                           "HEAD")))
                                     (funcall
                                      git
                                      "remote"
                                      "add"
                                      "origin"
                                      (expand-file-name
                                       "unreachable-origin.git"
                                       root))
                                     (funcall
                                      git
                                      "update-ref"
                                      "refs/remotes/origin/main"
                                      main-commit))
                                   (funcall
                                    git
                                    "switch"
                                    "-c"
                                    "feature/parser-recovery")
                                   (with-temp-file
                                       source
                                     (insert
                                      "(defun parser-state () 'recovered)\n"))
                                   (funcall
                                    git
                                    "add"
                                    "src/parser.el")
                                   (funcall
                                    git
                                    "commit"
                                    "-m"
                                    "Recover parser transitions"
                                    "-m"
                                    "Reset lookahead after recovery.\n\nPreserve request ordering across retries.")
                                   (agitjo--set-current-topic
                                    "team/parser-session")
                                   (let ((config
                                          (agitjo--pullreq-configuration
                                           :type
                                           "for"
                                           :source
                                           "feature/parser-recovery"
                                           :target
                                           "origin/main"
                                           :args
                                           '("draft"
                                             "--push-option=title=Parser recovery"))))
                                     (agitjo-post--setup-buffer
                                      config)
                                     (setq
                                      post-buffer
                                      (agitjo-post--buffer)
                                      draft-file
                                      (buffer-file-name
                                       post-buffer))
                                     (with-current-buffer
                                         post-buffer
                                       (let ((composed
                                              (list
                                               major-mode
                                               mode-name
                                               (substring-no-properties
                                                header-line-format)
                                               (buffer-string)
                                               (agitjo--pullreq-refspec
                                                agitjo-post--pullreq-config)
                                               (file-relative-name
                                                draft-file
                                                root)
                                               (buffer-modified-p))))
                                         (goto-char
                                          (point-max))
                                         (insert
                                          "\n\n## Verification\n\n"
                                          "- [x] Differential parser tests\n"
                                          "- [x] Retry ordering preserved\n")
                                         (agitjo-post-cancel)
                                         (setq
                                          result
                                          (list
                                           composed
                                           (current-message)
                                           (buffer-live-p
                                            post-buffer)
                                           (file-exists-p
                                            draft-file)
                                           (with-temp-buffer
                                             (insert-file-contents
                                              draft-file)
                                             (buffer-string))))))))
                               (when
                                   (buffer-live-p
                                    post-buffer)
                                 (with-current-buffer
                                     post-buffer
                                   (set-buffer-modified-p
                                    nil))
                                 (kill-buffer
                                  post-buffer))
                               (when
                                   (file-exists-p
                                    root)
                                 (delete-directory
                                  root
                                  t)))
                             result)"####,
        expect![[
            r#"OK ((agitjo-post-mode "AGitjo-Post" " C-c C-c to publish or C-c C-k to cancel." "Reset lookahead after recovery.\n\nPreserve request ordering across retries.\n\n-----\n\n## Summary\n\nExplain the user-visible effect.\n\n## Checklist\n\n- [ ] Tests pass\n" "feature/parser-recovery:refs/for/main/team/parser-session" ".git/agitjo/pullreq-draft" t) nil nil t "Reset lookahead after recovery.\n\nPreserve request ordering across retries.\n\n-----\n\n## Summary\n\nExplain the user-visible effect.\n\n## Checklist\n\n- [ ] Tests pass\n\n\n## Verification\n\n- [x] Differential parser tests\n- [x] Retry ordering preserved\n")"#
        ]],
    )
}

pub(super) fn composition_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![agitjo_composes_edits_and_cancels_a_real_template_backed_pull_request_draft()]
}
