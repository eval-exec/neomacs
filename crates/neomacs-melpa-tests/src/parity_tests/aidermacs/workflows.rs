use expect_test::expect;

use super::ParityBatchCase;

fn a_real_session_detects_each_edited_file_format_and_skips_the_one_that_does_not_exist()
-> ParityBatchCase {
    ParityBatchCase::value(
        "a_real_session_detects_each_edited_file_format_and_skips_the_one_that_does_not_exist",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (root (file-name-as-directory (expand-file-name "repo" sandbox)))
       (program (expand-file-name "bin/aider-standin" sandbox))
       (log (expand-file-name "argv.log" sandbox))
       (default-directory root)
       shown)
  (make-directory (expand-file-name ".git" root) t)
  (make-directory (expand-file-name "src" root) t)
  (make-directory (file-name-directory program) t)
  ;; Three files that exist on disk, and deliberately NOT src/ghost.el.
  ;; `aidermacs--detect-edited-files' announces four and must keep three:
  ;; three and three would agree with a detector that never filtered.
  (dolist (relative '("src/real.el" "src/block.el" "src/diff.el"))
    (write-region (format ";;; %s\n(defun placeholder () nil)\n" relative)
                  nil (expand-file-name relative root) nil 'silent))
  ;; The stand-in speaks aider's three edit-announcement shapes at once:
  ;; an "Applied edit to" line, a fenced block preceded by its filename,
  ;; and a udiff header pair.  The LLM behind aider is a true external
  ;; service, so this text is a format contract derived from the
  ;; package's own parser rather than a recording; the command line the
  ;; package builds, asserted below, IS checkable and was checked against
  ;; aider 0.86.1, which accepts --no-pretty and --no-fancy-input.
  (write-region
   (concat "#!/bin/sh\n"
           "{ printf 'argv:'; for a in \"$@\"; do printf ' [%s]' \"$a\"; done; printf '\\n'; } >> \"$AID_LOG\"\n"
           "printf '> '\n"
           "while IFS= read -r line; do\n"
           "  cat <<'REPLY'\n"
           "Applied edit to ./src/real.el\n"
           "Applied edit to ./src/ghost.el\n"
           "src/block.el\n"
           "```elisp\n"
           "(defun added () t)\n"
           "```\n"
           "--- src/diff.el\n"
           "+++ src/diff.el\n"
           "@@ -1,2 +1,3 @@\n"
           "REPLY\n"
           "  printf '> '\n"
           "done\n")
   nil program nil 'silent)
  (set-file-modes program #o755)
  (setenv "AID_LOG" log)
  (write-region "" nil log nil 'silent)
  (let ((buffer-name (aidermacs-get-buffer-name)))
    (unwind-protect
        ;; Only the ediff *display* is stood in for -- it opens windows.
        ;; The detection itself runs for real, and what it handed over is
        ;; exactly the product being asserted.
        (cl-letf (((symbol-function 'aidermacs--show-ediff-for-edited-files)
                   (lambda (files)
                     (setq shown (append shown (list (mapcar #'copy-sequence files))))
                     'shown)))
          (aidermacs-run-comint program '("--model" "standin") buffer-name)
          (with-current-buffer buffer-name
            (let ((aidermacs-enable-notifications nil)
                  (attempts 0))
              (while (and (not aidermacs--ready) (< attempts 300))
                (accept-process-output nil 0.02)
                (setq attempts (1+ attempts)))
              (setq-local aidermacs--ready nil)
              (aidermacs--send-command-comint (current-buffer) "/add src/real.el")
              (setq attempts 0)
              (while (and (not aidermacs--ready) (< attempts 300))
                (accept-process-output nil 0.02)
                (setq attempts (1+ attempts)))
              (list :ready aidermacs--ready
                    :mode major-mode
                    :announced 4
                    :detected shown
                    ;; The argument vector the PROCESS received.  The two
                    ;; trailing flags are appended by `aidermacs-run-comint'
                    ;; itself, so a test that stubs the backend never sees
                    ;; them however carefully it asserts the built args.
                    :argv (with-temp-buffer
                            (insert-file-contents log)
                            (buffer-string))))))
      (when-let ((buffer (get-buffer buffer-name)))
        (when-let ((process (get-buffer-process buffer)))
          (delete-process process))
        (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (:ready t :mode aidermacs-comint-mode :announced 4 :detected (("src/real.el" "src/block.el" "src/diff.el")) :argv "argv: [--model] [standin] [--no-pretty] [--no-fancy-input]\n")"#
        ]],
    )
}

fn answering_an_aider_question_sends_the_analysis_request_back_automatically() -> ParityBatchCase {
    ParityBatchCase::value(
        "answering_an_aider_question_sends_the_analysis_request_back_automatically",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (root (file-name-as-directory (expand-file-name "repo" sandbox)))
       (program (expand-file-name "bin/aider-standin" sandbox))
       (received (expand-file-name "received.log" sandbox))
       (default-directory root))
  (make-directory (expand-file-name ".git" root) t)
  (make-directory (file-name-directory program) t)
  ;; The first reply is a QUESTION and carries no prompt, so the package
  ;; must recognise it through `aidermacs-question-regexp' rather than
  ;; through the prompt; the second reply returns to a prompt, which is
  ;; when the deferred analysis request is due.
  (write-region
   (concat "#!/bin/sh\n"
           "printf '> '\n"
           "count=0\n"
           "while IFS= read -r line; do\n"
           "  printf '%s\\n' \"$line\" >> \"$AID_RECEIVED\"\n"
           "  count=$((count+1))\n"
           "  if [ $count = 1 ]; then\n"
           "    printf 'Running: ls -la\\nAdd command output to the chat? (Y)es/(N)o [Yes]: '\n"
           "  else\n"
           "    printf 'ok\\n> '\n"
           "  fi\n"
           "done\n")
   nil program nil 'silent)
  (set-file-modes program #o755)
  (setenv "AID_RECEIVED" received)
  (write-region "" nil received nil 'silent)
  (let ((buffer-name (aidermacs-get-buffer-name)))
    (unwind-protect
        (progn
          (aidermacs-run-comint program '("--model" "standin") buffer-name)
          (with-current-buffer buffer-name
            (let ((aidermacs-enable-notifications nil)
                  (aidermacs-show-diff-after-change nil)
                  (attempts 0))
              (while (and (not aidermacs--ready) (< attempts 300))
                (accept-process-output nil 0.02)
                (setq attempts (1+ attempts)))
              (setq-local aidermacs--ready nil)
              (aidermacs--send-command-comint (current-buffer) "/run ls -la")
              (setq attempts 0)
              (while (and (not aidermacs--ready) (< attempts 300))
                (accept-process-output nil 0.02)
                (setq attempts (1+ attempts)))
              (let ((awaiting-after-question
                     (and aidermacs--comint-awaiting-output-analysis t)))
                (setq-local aidermacs--ready nil)
                (aidermacs--send-command-comint (current-buffer) "y")
                (setq attempts 0)
                (while (and (not aidermacs--ready) (< attempts 300))
                  (accept-process-output nil 0.02)
                  (setq attempts (1+ attempts)))
                (accept-process-output nil 0.2)
                (list :ready aidermacs--ready
                      :awaiting-after-question awaiting-after-question
                      :awaiting-cleared
                      (null aidermacs--comint-awaiting-output-analysis)
                      ;; What aider actually received: the user's two
                      ;; lines, and then a third the package sent by
                      ;; itself once the prompt came back.
                      :received (with-temp-buffer
                                  (insert-file-contents received)
                                  (buffer-string)))))))
      (when-let ((buffer (get-buffer buffer-name)))
        (when-let ((process (get-buffer-process buffer)))
          (delete-process process))
        (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (:ready t :awaiting-after-question t :awaiting-cleared t :received "/run ls -la\ny\nPlease analyze the command output above.\n")"#
        ]],
    )
}

fn the_community_edition_only_flag_is_withheld_from_a_stock_aider() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_community_edition_only_flag_is_withheld_from_a_stock_aider",
        r##"(let ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  (cl-flet
      ((session-args (binary-name)
         (let* ((root (file-name-as-directory
                       (expand-file-name (concat "repo-" binary-name) sandbox)))
                (program (expand-file-name (concat "bin/" binary-name) sandbox))
                (default-directory root)
                (aidermacs-program program)
                (aidermacs--resolved-programs (make-hash-table :test 'equal))
                (aidermacs--cached-versions (make-hash-table :test 'equal))
                captured)
           (make-directory (expand-file-name ".git" root) t)
           (make-directory (file-name-directory program) t)
           (write-region "#!/bin/sh\nprintf 'aider 0.86.1\\n'\n"
                         nil program nil 'silent)
           (set-file-modes program #o755)
           (cl-letf (((symbol-function 'aidermacs-run-backend)
                      (lambda (_program args buffer-name)
                        (setq captured args)
                        (get-buffer-create buffer-name)))
                     ((symbol-function 'aidermacs-switch-to-buffer) #'ignore)
                     ((symbol-function 'aidermacs--setup-ediff-cleanup-hooks) #'ignore)
                     ((symbol-function 'aidermacs--setup-cleanup-hooks) #'ignore)
                     ((symbol-function 'aidermacs-setup-minor-mode) #'ignore))
             (let ((aidermacs-default-chat-mode nil)
                   (aidermacs-default-model "m")
                   (aidermacs-auto-commits t)
                   (aidermacs-watch-files nil)
                   (aidermacs-weak-model nil)
                   (aidermacs-global-read-only-files nil)
                   (aidermacs-project-read-only-files nil)
                   (aidermacs-extra-args nil))
               (aidermacs-run))
             (mapc #'kill-buffer
                   (match-buffers
                    (lambda (buffer)
                      (string-prefix-p "*aidermacs:" (buffer-name buffer)))))
             ;; Both arms build their arguments from the same string
             ;; objects, so an uncopied capture renders the second arm as
             ;; `#1#' back references into the first.
             (list (copy-sequence binary-name)
                   (mapcar #'copy-sequence captured)
                   (and (member "--linear-output" captured) t))))))
    (list :stock (session-args "aider")
          :community-edition (session-args "aider-ce"))))"##,
        expect![[
            r#"OK (:stock ("aider" ("--model" "m" "--no-auto-accept-architect") nil) :community-edition ("aider-ce" ("--model" "m" "--no-auto-accept-architect" "--linear-output") t))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_real_session_detects_each_edited_file_format_and_skips_the_one_that_does_not_exist(),
        answering_an_aider_question_sends_the_analysis_request_back_automatically(),
        the_community_edition_only_flag_is_withheld_from_a_stock_aider(),
    ]
}
