use expect_test::expect;

use super::ParityBatchCase;

fn anyins_documented_yank_fills_an_irregular_status_column_from_the_current_point()
-> ParityBatchCase {
    ParityBatchCase::value(
        "anyins_documented_yank_fills_an_irregular_status_column_from_the_current_point",
        r##"(with-temp-buffer
  (insert "Name Status\nAda\nBob\nCy")
  (goto-char (point-min))
  (search-forward "Ada")
  (let ((kill-ring
         '(" | active\n | invited\n | disabled")))
    (let* ((enable-result (anyins-mode 1))
           (yank-command (key-binding (kbd "y")))
           (enabled-state
            (list
             :result enable-result
             :mode anyins-mode
             :read-only buffer-read-only))
           (yank-result
            (call-interactively yank-command)))
      (list
       :yank-binding yank-command
       :enabled enabled-state
       :yank-result yank-result
       :document
       (buffer-substring (point-min) (point-max))
       :point (point)
       :finished
       (list
        :mode anyins-mode
        :read-only buffer-read-only
        :highlight-count
        (neomacs-anyins-highlight-count))))))"##,
        expect![[
            r#"OK (:yank-binding anyins-yank :enabled (:result t :mode t :read-only t) :yank-result nil :document "Name Status\nAda | active\nBob | invited\nCy  | disabled" :point 54 :finished (:mode nil :read-only nil :highlight-count 0))"#
        ]],
    )
}

fn anyins_marked_yank_prefixes_multiple_fields_per_line_in_recording_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "anyins_marked_yank_prefixes_multiple_fields_per_line_in_recording_order",
        r##"(with-temp-buffer
  (insert "host port\napi 443\ncache 6379")
  (let ((kill-ring
         '("server=\nnumber=\nserver=\nnumber=\nserver=\nnumber="))
        marked-points
        duplicate-state)
    (anyins-mode 1)
    (goto-char (point-min))
    (dolist
        (field
         '("host" "port" "api" "443" "cache" "6379"))
      (search-forward field)
      (goto-char (match-beginning 0))
      (push (point) marked-points)
      (call-interactively (key-binding (kbd "RET")))
      (unless duplicate-state
        (call-interactively (key-binding (kbd "RET")))
        (setq duplicate-state
              (neomacs-anyins-marker-state
               (list (point)))))
      (goto-char (match-end 0)))
    (setq marked-points (nreverse marked-points))
    (let ((marked-state
           (neomacs-anyins-marker-state
            marked-points))
          (yank-command (key-binding (kbd "y"))))
      (call-interactively yank-command)
      (list
       :yank-binding yank-command
       :duplicate-mark duplicate-state
       :marked marked-state
       :document
       (buffer-substring (point-min) (point-max))
       :point (point)
       :finished
       (list
        :mode anyins-mode
        :read-only buffer-read-only
        :highlight-count
        (neomacs-anyins-highlight-count))))))"##,
        expect![[
            r#"OK (:yank-binding anyins-yank :duplicate-mark (:points (1) :faces (anyins-recorded-positions) :highlight-count 1 :read-only t) :marked (:points (1 6 11 15 19 25) :faces (anyins-recorded-positions anyins-recorded-positions anyins-recorded-positions anyins-recorded-positions anyins-recorded-positions anyins-recorded-positions) :highlight-count 6 :read-only t) :document "server=host number=port\nserver=api number=443\nserver=cache number=6379" :point 67 :finished (:mode nil :read-only nil :highlight-count 0))"#
        ]],
    )
}

fn anyins_shell_workflow_runs_a_real_generator_through_the_interactive_command() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anyins_shell_workflow_runs_a_real_generator_through_the_interactive_command",
        r##"(let* ((root
         (file-name-as-directory
          (expand-file-name
           "anyins-shell-workflow"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (script (expand-file-name "generate-statuses" root))
        (trace (expand-file-name "generator.trace" root)))
  (unwind-protect
      (progn
        (make-directory root t)
        (with-temp-file script
          (insert
           "#!/bin/sh\n"
           "set -eu\n"
           "printf '%s\\n' \"$1\" > \"$NEOMACS_ANYINS_TRACE\"\n"
           "printf '%s\\n' ' | active' ' | paused' ' | active'\n"))
        (set-file-modes script #o755)
        (setenv "NEOMACS_ANYINS_TRACE" trace)
        (save-window-excursion
          (with-temp-buffer
            (switch-to-buffer (current-buffer))
            (insert "api\nworker\nscheduler")
            (let (marked-points
                  prompt
                  invocation)
              (anyins-mode 1)
              (goto-char (point-min))
              (dolist (service '("api" "worker" "scheduler"))
                (search-forward service)
                (push (point) marked-points)
                (call-interactively (key-binding (kbd "RET"))))
              (setq marked-points (nreverse marked-points))
              (let* ((marked-state
                      (neomacs-anyins-marker-state
                       marked-points))
                     (command
                      (concat
                       (shell-quote-argument script)
                       " production"))
                     (insert-command
                      (key-binding (kbd "!"))))
                (let ((command-history nil)
                      (minibuffer-setup-hook
                       (list
                        (lambda ()
                          (setq prompt
                                (minibuffer-prompt))))))
                  (execute-kbd-macro
                   (concat "!" command "\r"))
                  (setq invocation (car command-history)))
                (list
                 :insert-binding insert-command
                 :prompt prompt
                 :answer command
                 :invocation invocation
                 :marked marked-state
                 :document
                 (buffer-substring
                  (point-min) (point-max))
                 :point (point)
                 :generator-trace
                 (if (file-exists-p trace)
                     (with-temp-buffer
                       (insert-file-contents-literally trace)
                       (buffer-string))
                   :missing)
                 :finished
                 (list
                  :mode anyins-mode
                  :read-only buffer-read-only
                  :highlight-count
                  (neomacs-anyins-highlight-count))))))))
    (setenv "NEOMACS_ANYINS_TRACE" nil)
    (when (file-exists-p root)
      (delete-directory root t))))"##,
        expect![[
            r#"OK (:insert-binding anyins-insert-command :prompt "Shell command: " :answer "[ORACLE-SANDBOX]/anyins-shell-workflow/generate-statuses production" :invocation (anyins-insert-command "[ORACLE-SANDBOX]/anyins-shell-workflow/generate-statuses production") :marked (:points (4 11 21) :faces (anyins-recorded-positions anyins-recorded-positions nil) :highlight-count 2 :read-only t) :document "api | active\nworker | paused\nscheduler | active" :point 48 :generator-trace "production\n" :finished (:mode nil :read-only nil :highlight-count 0))"#
        ]],
    )
}

fn anyins_shell_output_cardinality_handles_exact_short_and_long_real_process_results()
-> ParityBatchCase {
    ParityBatchCase::value(
        "anyins_shell_output_cardinality_handles_exact_short_and_long_real_process_results",
        r##"(let* ((root
         (file-name-as-directory
          (expand-file-name
           "anyins-shell-cardinality"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (script (expand-file-name "generate-prefixes" root))
        (trace (expand-file-name "generator.trace" root)))
  (unwind-protect
      (progn
        (make-directory root t)
        (with-temp-file script
          (insert
           "#!/bin/sh\n"
           "set -eu\n"
           "printf '%s\\n' \"$1\" >> \"$NEOMACS_ANYINS_TRACE\"\n"
           "case \"$1\" in\n"
           "  exact) printf '1.\\n2.\\n3.\\n' ;;\n"
           "  short) printf '1.\\n2.\\n' ;;\n"
           "  long) printf '1.\\n2.\\n3.\\n4.\\n' ;;\n"
           "  *) exit 64 ;;\n"
           "esac\n"))
        (set-file-modes script #o755)
        (setenv "NEOMACS_ANYINS_TRACE" trace)
        (let (phases)
          (dolist
              (case
               '(("exact" "alpha\nbeta\ngamma")
                 ("short" "alpha\nbeta\ngamma\ndelta")
                 ("long" "alpha\nbeta")))
            (let ((name (car case))
                  (document (cadr case)))
              (push
               (with-temp-buffer
                 (insert document)
                 (goto-char (point-min))
                 (let ((command
                        (concat
                         (shell-quote-argument script)
                         " "
                         name)))
                   (anyins-mode 1)
                   (anyins-insert-command command)
                   (list
                    :case name
                    :command command
                    :document
                    (buffer-substring
                     (point-min) (point-max))
                    :point (point)
                    :finished
                    (list
                     :mode anyins-mode
                     :read-only buffer-read-only
                     :highlight-count
                     (neomacs-anyins-highlight-count)))))
               phases)))
          (list
           :phases (nreverse phases)
           :generator-trace
           (with-temp-buffer
             (insert-file-contents-literally trace)
             (buffer-string)))))
    (setenv "NEOMACS_ANYINS_TRACE" nil)
    (when (file-exists-p root)
      (delete-directory root t))))"##,
        expect![[
            r#"OK (:phases ((:case "exact" :command "[ORACLE-SANDBOX]/anyins-shell-cardinality/generate-prefixes exact" :document "1.alpha\n2.beta\n3.gamma" :point 18 :finished (:mode nil :read-only nil :highlight-count 0)) (:case "short" :command "[ORACLE-SANDBOX]/anyins-shell-cardinality/generate-prefixes short" :document "1.alpha\n2.beta\ngamma\ndelta" :point 22 :finished (:mode nil :read-only nil :highlight-count 0)) (:case "long" :command "[ORACLE-SANDBOX]/anyins-shell-cardinality/generate-prefixes long" :document "1.alpha\n2.beta" :point 11 :finished (:mode nil :read-only nil :highlight-count 0))) :generator-trace "exact\nshort\nlong\n")"#
        ]],
    )
}

fn anyins_abort_key_leaves_the_document_untouched_and_removes_every_marker() -> ParityBatchCase {
    ParityBatchCase::value(
        "anyins_abort_key_leaves_the_document_untouched_and_removes_every_marker",
        r##"(with-temp-buffer
  (insert
   "A computer is a general purpose device.\n"
   "It performs arithmetic operations.\n"
   "A sequence can be changed safely.")
  (let ((original (buffer-string))
        marked-points
        other-buffer-state)
    (anyins-mode 1)
    (goto-char (point-min))
    (dolist (needle '("general" "arithmetic" "sequence"))
      (search-forward needle)
      (goto-char (match-beginning 0))
      (push (point) marked-points)
      (call-interactively (key-binding (kbd "RET")))
      (goto-char (match-end 0)))
    (setq marked-points (nreverse marked-points))
    (setq
     other-buffer-state
     (with-temp-buffer
       (insert "isolated document")
       (anyins-mode 1)
       (goto-char (point-min))
       (search-forward "document")
       (goto-char (match-beginning 0))
       (call-interactively (key-binding (kbd "RET")))
       (let ((marked
              (neomacs-anyins-marker-state
               (list (point))))
             (document
              (buffer-substring
               (point-min) (point-max))))
         (call-interactively (key-binding (kbd "q")))
         (list
          :document document
          :marked marked
          :finished
          (list
           :mode anyins-mode
           :read-only buffer-read-only
           :highlight-count
           (neomacs-anyins-highlight-count))))))
    (let ((marked-state
           (neomacs-anyins-marker-state
            marked-points))
          (abort-command (key-binding (kbd "q"))))
      (call-interactively abort-command)
      (list
       :abort-binding abort-command
       :other-buffer other-buffer-state
       :marked marked-state
       :unchanged (equal original (buffer-string))
       :document
       (buffer-substring (point-min) (point-max))
       :point (point)
       :finished
       (list
        :mode anyins-mode
        :read-only buffer-read-only
        :highlight-count
        (neomacs-anyins-highlight-count))))))"##,
        expect![[
            r#"OK (:abort-binding anyins-disable-mode :other-buffer (:document "isolated document" :marked (:points (10) :faces (anyins-recorded-positions) :highlight-count 1 :read-only t) :finished (:mode nil :read-only nil :highlight-count 0)) :marked (:points (17 53 78) :faces (anyins-recorded-positions anyins-recorded-positions anyins-recorded-positions) :highlight-count 3 :read-only t) :unchanged t :document "A computer is a general purpose device.\nIt performs arithmetic operations.\nA sequence can be changed safely." :point 86 :finished (:mode nil :read-only nil :highlight-count 0))"#
        ]],
    )
}

pub(super) fn practical_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anyins_documented_yank_fills_an_irregular_status_column_from_the_current_point(),
        anyins_marked_yank_prefixes_multiple_fields_per_line_in_recording_order(),
        anyins_shell_workflow_runs_a_real_generator_through_the_interactive_command(),
        anyins_shell_output_cardinality_handles_exact_short_and_long_real_process_results(),
        anyins_abort_key_leaves_the_document_untouched_and_removes_every_marker(),
    ]
}
