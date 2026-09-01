use expect_test::expect;

use super::ParityBatchCase;

fn agitjo_publish_preserves_a_failed_real_draft_then_retries_and_clears_it_on_success()
-> ParityBatchCase {
    ParityBatchCase::value(
        "agitjo_publish_preserves_a_failed_real_draft_then_retries_and_clears_it_on_success",
        r####"(let* ((root
                                  (file-name-as-directory
                                   (expand-file-name
                                    "agitjo-publish"
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
                                 (description-prefix
                                  "--push-option=description=")
                                 (normalize-args
                                  (lambda (arguments)
                                    (mapcar
                                     (lambda (argument)
                                       (if
                                           (string-prefix-p
                                            description-prefix
                                            argument)
                                           (list
                                            :description
                                            (decode-coding-string
                                             (base64-decode-string
                                              (substring
                                               argument
                                               (+
                                                (length
                                                 description-prefix)
                                                (length
                                                 "{base64}"))))
                                             'utf-8-unix))
                                         argument))
                                     arguments)))
                                 (agitjo--current-topics
                                  nil)
                                 requests
                                 sentinel-events
                                 process
                                 draft-buffer
                                 retry-buffer
                                 draft-file
                                 failed-file
                                 failed-message
                                 failed-buffer-live
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
                                    root
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
                                       (expand-file-name
                                        "parser.el"
                                        root)
                                     (insert
                                      "(defun parser-ready-p () nil)\n"))
                                   (funcall
                                    git
                                    "add"
                                    "parser.el")
                                   (funcall
                                    git
                                    "commit"
                                    "-m"
                                    "Initial parser")
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
                                    "feature/parser")
                                   (with-temp-file
                                       (expand-file-name
                                        "parser.el"
                                        root)
                                     (insert
                                      "(defun parser-ready-p () t)\n"))
                                   (funcall
                                    git
                                    "add"
                                    "parser.el")
                                   (funcall
                                    git
                                    "commit"
                                    "-m"
                                    "Make parser ready")
                                   (agitjo--set-current-topic
                                    "team/retry-42")
                                   (let ((config
                                          (agitjo--pullreq-configuration
                                           :type
                                           "for"
                                           :source
                                           "feature/parser"
                                           :target
                                           "origin/main"
                                           :args
                                           '("draft"
                                             "--push-option=title=Parser recovery"
                                             "--porcelain"))))
                                     (setq
                                      draft-buffer
                                      (agitjo-post--buffer)
                                      draft-file
                                      (buffer-file-name
                                       draft-buffer))
                                     (switch-to-buffer
                                      draft-buffer)
                                     (agitjo-post-mode)
                                     (setq-local
                                      agitjo-post--pullreq-config
                                      config)
                                     (insert
                                      "Parser recovery now handles café input.\n\n"
                                      "## Verification\n\n"
                                      "- [x] Unicode payload\n"
                                      "- [x] Retry ordering\n")
                                     (cl-letf
                                         (((symbol-function
                                            'magit-run-git-async)
                                           (lambda (&rest arguments)
                                             (push
                                              arguments
                                              requests)
                                             (setq
                                              process
                                              (make-process
                                               :name
                                               "agitjo-test-failed-push"
                                               :command
                                               '("sh"
                                                 "-c"
                                                 "sleep 0.05; exit 9")
                                               :connection-type
                                               'pipe
                                               :noquery
                                               t))
                                             process))
                                          ((symbol-function
                                            'magit-process-sentinel)
                                           (lambda (proc event)
                                             (push
                                              (list
                                               (process-name
                                                proc)
                                               (process-status
                                                proc)
                                               (process-exit-status
                                                proc)
                                               event)
                                              sentinel-events))))
                                       (with-current-buffer
                                           draft-buffer
                                         (agitjo-post-confirm))
                                       (while
                                           (process-live-p
                                            process)
                                         (accept-process-output
                                          process
                                          0.05))
                                       (accept-process-output
                                        process
                                        0.05))
                                     (setq
                                      failed-message
                                      (current-message)
                                      failed-buffer-live
                                      (buffer-live-p
                                       draft-buffer)
                                      failed-file
                                      (with-temp-buffer
                                        (insert-file-contents
                                         draft-file)
                                        (buffer-string))
                                      retry-buffer
                                      (find-file-noselect
                                       draft-file))
                                     (switch-to-buffer
                                      retry-buffer)
                                     (agitjo-post-mode)
                                     (setq-local
                                      agitjo-post--pullreq-config
                                      config)
                                     (goto-char
                                      (point-max))
                                     (insert
                                      "- [x] Retry after transient failure\n")
                                     (cl-letf
                                         (((symbol-function
                                            'magit-run-git-async)
                                           (lambda (&rest arguments)
                                             (push
                                              arguments
                                              requests)
                                             (setq
                                              process
                                              (make-process
                                               :name
                                               "agitjo-test-successful-push"
                                               :command
                                               '("sh"
                                                 "-c"
                                                 "sleep 0.05; exit 0")
                                               :connection-type
                                               'pipe
                                               :noquery
                                               t))
                                             process))
                                          ((symbol-function
                                            'magit-process-sentinel)
                                           (lambda (proc event)
                                             (push
                                              (list
                                               (process-name
                                                proc)
                                               (process-status
                                                proc)
                                               (process-exit-status
                                                proc)
                                               event)
                                              sentinel-events))))
                                       (with-current-buffer
                                           retry-buffer
                                         (agitjo-post-confirm))
                                       (while
                                           (process-live-p
                                            process)
                                         (accept-process-output
                                          process
                                          0.05))
                                       (accept-process-output
                                        process
                                        0.05))
                                     (setq
                                      result
                                      (list
                                       (agitjo--pullreq-refspec
                                        config)
                                       (agitjo--pullreq-target-remote
                                        config)
                                       failed-message
                                       failed-buffer-live
                                       failed-file
                                       (buffer-live-p
                                        retry-buffer)
                                       (file-exists-p
                                        draft-file)
                                       (with-temp-buffer
                                         (insert-file-contents
                                          draft-file)
                                         (buffer-string))
                                       (mapcar
                                        (lambda (request)
                                          (append
                                           (butlast
                                            request)
                                           (list
                                            (funcall
                                             normalize-args
                                             (car
                                              (last
                                               request))))))
                                        (nreverse
                                         requests))
                                       (nreverse
                                        sentinel-events)
                                       (funcall
                                        normalize-args
                                        (oref
                                         config
                                         args))))))
                               (when
                                   (process-live-p
                                    process)
                                 (delete-process
                                  process))
                               (dolist (buffer
                                        (list
                                         draft-buffer
                                         retry-buffer))
                                 (when
                                     (buffer-live-p
                                      buffer)
                                   (with-current-buffer
                                       buffer
                                     (set-buffer-modified-p
                                      nil))
                                   (kill-buffer
                                    buffer)))
                               (when
                                   (file-exists-p
                                    root)
                                 (delete-directory
                                  root
                                  t)))
                             result)"####,
        expect![[
            r#"OK ("feature/parser:refs/for/main/team/retry-42" "origin" nil nil "Parser recovery now handles café input.\n\n## Verification\n\n- [x] Unicode payload\n- [x] Retry ordering\n" nil t "" (("push" "-v" "origin" "feature/parser:refs/for/main/team/retry-42" ((:description "Parser recovery now handles café input.\n\n## Verification\n\n- [x] Unicode payload\n- [x] Retry ordering\n") "--push-option=title=WIP: Parser recovery" "--porcelain")) ("push" "-v" "origin" "feature/parser:refs/for/main/team/retry-42" ((:description "Parser recovery now handles café input.\n\n## Verification\n\n- [x] Unicode payload\n- [x] Retry ordering\n- [x] Retry after transient failure\n") "--push-option=title=WIP: Parser recovery" "--porcelain"))) (("agitjo-test-failed-push" exit 9 "exited abnormally with code 9\n") ("agitjo-test-successful-push" exit 0 "finished\n")) ((:description "Parser recovery now handles café input.\n\n## Verification\n\n- [x] Unicode payload\n- [x] Retry ordering\n- [x] Retry after transient failure\n") "draft" "--push-option=title=Parser recovery" "--porcelain"))"#
        ]],
    )
}

pub(super) fn publish_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![agitjo_publish_preserves_a_failed_real_draft_then_retries_and_clears_it_on_success()]
}
