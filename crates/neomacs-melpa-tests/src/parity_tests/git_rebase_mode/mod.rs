//! Practical parity for the retired standalone git-rebase-mode package.
//!
//! MELPA 20150122.1914 is reproduced from the recipe-selected file at
//! upstream first-parent merge `acccc25f5207cfa93fe3faf36d315bdc1cecebfc`.
//! The corpus exercises the historical package itself, not the modern mode
//! bundled with Magit.
//!
//! The fallback-insert workflow deliberately retains one Neo core red.  This
//! package passes a valid prefix, a list returned by `process-lines`, and a
//! newline to primitive `insert`.  GNU inserts the valid prefix before it
//! rejects the list; Neo validates all arguments before changing the buffer.
//! The workflow applies public undo when needed, then both editors recover
//! through the documented Magit integration and produce identical final
//! buffers and external traces.
//!
//! The Git boundary replay was recorded from Git 2.51.2 (executable SHA-256
//! `f01676568f1dc06110d91eb3923ba069338c0cada4b5798b225170991363c352`).
//! A deterministic empty-tree repository, with author and committer identity
//! `Test User <test@example.invalid>` and timestamp
//! `2015-01-22T19:14:15Z`, produced commit
//! `a9be7ac870bd756a3fde9041bf9385c23642ec4a`.  The replay below preserves
//! the exact stdout recorded from the two public package invocations.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIT_REBASE_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'git-rebase-mode)

;; Establish lazy mode-owned tables and hook effects before shared baselines.
(with-temp-buffer (git-rebase-mode))

(defvar git383-test-owned-roots nil)

(defconst git383-test-todo
  (concat
   "pick a1b2c3d prepare café\n"
   "reword b2c3d4e explain λ\n"
   "exec printf '界'\n"
   "#fixup c3d4e5f folded Ω\n"
   "# Rebase commands:\n"
   "#  p, pick = use commit\n"
   "#  r, reword = edit message\n"))

(defun git383-test-face-runs ()
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (or (next-single-property-change
                        position 'face nil (point-max))
                       (point-max))))
        (when face
          (push (list position next face
                      (buffer-substring-no-properties position next))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun git383-test-overlay-runs ()
  (mapcar
   (lambda (overlay)
     (list (overlay-start overlay) (overlay-end overlay)
           :display (overlay-get overlay 'display)
           :text (buffer-substring-no-properties
                  (overlay-start overlay) (overlay-end overlay))))
   (sort (overlays-in (point-min) (point-max))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun git383-test-buffer-state ()
  (list :mode major-mode :name mode-name :read-only buffer-read-only
        :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point) :line (line-number-at-pos) :column (current-column)
        :modified (buffer-modified-p)
        :undo (cond ((eq buffer-undo-list t) 'disabled)
                    (buffer-undo-list 'present)
                    (t nil))))

(defun git383-test-line-state ()
  (list :line (line-number-at-pos) :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defmacro git383-test-with-inputs (inputs &rest body)
  (declare (indent 1))
  `(let* ((input-specs ,inputs)
          (executing-kbd-macro t)
          (unread-command-events
           (apply #'append
                  (mapcar
                   (lambda (input-spec)
                     (append (make-list (length (car input-spec)) 127)
                             (string-to-list (cdr input-spec))
                             (listify-key-sequence (kbd "RET"))))
                   input-specs)))
          minibuffers result)
     (let ((minibuffer-setup-hook
            (cons (lambda ()
                    (push (list :prompt (minibuffer-prompt)
                                :initial (minibuffer-contents-no-properties))
                          minibuffers))
                  minibuffer-setup-hook)))
       (setq result
             (with-timeout
                 (5 (error "git-rebase minibuffer timed out: %S"
                           (car minibuffers)))
               ,@body)))
     (unless (and (= (length minibuffers) (length input-specs))
                  (null unread-command-events))
       (error "git-rebase input mismatch: %S %S"
              minibuffers unread-command-events))
     (list :value result :minibuffers (nreverse minibuffers))))

(defun git383-test-write-git (root)
  (let* ((bin (expand-file-name "bin" root))
         (tool (expand-file-name "git" bin)))
    (make-directory bin t)
    (write-region
     (concat
      "#!/bin/sh\n"
      "printf 'argv' >>\"$GIT383_LOG\"\n"
      "for arg in \"$@\"; do printf '<%s>' \"$arg\" >>\"$GIT383_LOG\"; done\n"
      "printf '\\n' >>\"$GIT383_LOG\"\n"
      "if [ \"$#\" -eq 4 ] && [ \"$1\" = show ] && [ \"$2\" = -s ] && "
      "[ \"$3\" = '--format=%h %s' ] && [ \"$4\" = 'topic/界' ]; then\n"
      "  printf 'a9be7ac inserted fallback\\n'\n"
      "elif [ \"$#\" -eq 2 ] && [ \"$1\" = show ] && "
      "[ \"$2\" = a9be7ac ]; then\n"
      "  printf 'commit a9be7ac870bd756a3fde9041bf9385c23642ec4a\\nAuthor: Test User <test@example.invalid>\\nDate:   Thu Jan 22 19:14:15 2015 +0000\\n\\n    inserted fallback\\n'\n"
      "else\n"
      "  printf 'UNRECORDED\\n' >>\"$GIT383_LOG\"\n"
      "  exit 86\n"
      "fi\n")
     nil tool nil 'silent)
    (set-file-modes tool #o700)
    tool))

(defun git383-test-read-file (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))

(defun git383-test-make-client (name buffer)
  (let ((process
         (make-pipe-process :name name :buffer nil :noquery t)))
    (process-put process 'buffers (list buffer))
    (process-put process 'no-delete-terminal t)
    (with-current-buffer buffer
      (setq-local server-buffer-clients (list process))
      (setq-local server-existing-buffer t))
    process))

(defun git383-test-server-result (buffer process path)
  (with-current-buffer buffer
    (list :buffer-live (buffer-live-p buffer)
          :clients server-buffer-clients
          :modified (buffer-modified-p)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :file (git383-test-read-file path)
          :process-live (process-live-p process)
          :process-member (memq process server-clients))))

(defun git383-test-run (name body)
  (let* ((root (make-temp-file (concat "git-rebase383-" name "-") t))
         (buffers-before (buffer-list))
         (frames-before (frame-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (buffer-before (current-buffer))
         (windows-before (current-window-configuration))
         (git383-test-owned-roots (list root))
         result body-error cleanup-errors)
    (unwind-protect
        (condition-case error
            (setq result (funcall body root))
          (error (setq body-error error)))
      (condition-case error
          (progn
            (when (buffer-live-p buffer-before) (set-buffer buffer-before))
            (set-window-configuration windows-before))
        (error (push (list :restore-windows error) cleanup-errors)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (progn
                (set-process-query-on-exit-flag process nil)
                (delete-process process))
            (error (push (list :delete-process (process-name process) error)
                         cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :kill-buffer (buffer-name buffer) error)
                         cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error
              (cancel-timer timer)
            (error (push (list :cancel-timer error) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case error
              (delete-frame frame t)
            (error (push (list :delete-frame error) cleanup-errors)))))
      (dolist (owned-root git383-test-owned-roots)
        (condition-case error
            (when (file-exists-p owned-root)
              (delete-directory owned-root t))
          (error (push (list :delete-root owned-root error) cleanup-errors))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (push (list :remaining-process (process-name process)) cleanup-errors)))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (push (list :remaining-buffer (buffer-name buffer)) cleanup-errors)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (push (list :remaining-timer t) cleanup-errors)))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (push (list :remaining-frame frame) cleanup-errors)))
      (dolist (owned-root git383-test-owned-roots)
        (when (file-exists-p owned-root)
          (push (list :remaining-root owned-root) cleanup-errors))))
    (cond
     ((and body-error cleanup-errors)
      (error "git-rebase body failed %S; cleanup failed %S"
             body-error (nreverse cleanup-errors)))
     (body-error (signal (car body-error) (cdr body-error)))
     (cleanup-errors
      (error "git-rebase cleanup failed: %S" (nreverse cleanup-errors)))
     (t (list :result result :cleanup 'clean)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIT_REBASE_MODE_MELPA_PIN, "git-rebase-mode.el")
        .expect("prepare exact historical git-rebase-mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn mode_activation_fontification_and_instruction_policy() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_activation_fontification_and_instruction_policy",
        r####"(git383-test-run
 "mode"
 (lambda (_root)
   (let ((source (symbol-file 'git-rebase-mode 'defun)) normal stripped)
     (with-temp-buffer
       (insert git383-test-todo)
       (goto-char (point-min))
       (git-rebase-mode)
       (font-lock-ensure)
       (setq normal
             (list :source-sha256
                   (secure-hash 'sha256
                                (with-temp-buffer
                                  (insert-file-contents-literally source)
                                  (buffer-string)))
                   :feature (featurep 'git-rebase-mode)
                   :mode (git383-test-buffer-state)
                   :keys (mapcar
                          (lambda (key) (cons key (key-binding (kbd key))))
                          '("c" "r" "e" "s" "f" "x" "y" "k"
                            "M-p" "M-n" "RET" "C-c C-c" "C-c C-k"))
                   :before-save-local (local-variable-p 'before-save-hook)
                   :before-save before-save-hook
                   :route (cdr (assoc "/git-rebase-todo\\'" auto-mode-alist))
                   :faces (git383-test-face-runs)
                   :overlays (git383-test-overlay-runs))))
     (with-temp-buffer
       (insert git383-test-todo)
       (goto-char (point-min))
       (let ((git-rebase-remove-instructions t))
         (git-rebase-mode))
       (setq stripped (git383-test-buffer-state)))
     (list :normal normal :stripped stripped))))"####,
        expect![[
            r##"OK (:result (:normal (:source-sha256 "52a996856e1c285bcc11bc22a129fa64edd19c71ee651373218d78ef990a77f9" :feature t :mode (:mode git-rebase-mode :name "Git Rebase" :read-only t :text "pick a1b2c3d prepare café\nreword b2c3d4e explain λ\nexec printf '界'\n#fixup c3d4e5f folded Ω\n# Rebase commands:\n#  p, pick = use commit\n#  r, reword = edit message\n" :point 1 :line 1 :column 0 :modified t :undo disabled) :keys (("c" . git-rebase-pick) ("r" . git-rebase-reword) ("e" . git-rebase-edit) ("s" . git-rebase-squash) ("f" . git-rebase-fixup) ("x" . git-rebase-exec) ("y" . git-rebase-insert) ("k" . git-rebase-kill-line) ("M-p" . git-rebase-move-line-up) ("M-n" . git-rebase-move-line-down) ("RET" . git-rebase-show-commit) ("C-c C-c" . git-rebase-server-edit) ("C-c C-k" . git-rebase-abort)) :before-save-local t :before-save nil :route git-rebase-mode :faces ((1 5 font-lock-keyword-face "pick") (6 13 git-rebase-hash "a1b2c3d") (14 26 git-rebase-description "prepare café") (27 33 font-lock-keyword-face "reword") (34 41 git-rebase-hash "b2c3d4e") (42 51 git-rebase-description "explain λ") (52 56 font-lock-keyword-face "exec") (68 91 git-rebase-killed-action "#fixup c3d4e5f folded Ω") (92 110 font-lock-comment-face "# Rebase commands:") (111 134 font-lock-comment-face "#  p, pick = use commit") (135 162 font-lock-comment-face "#  r, reword = edit message")) :overlays ((114 115 :display "c" :text "p") (138 139 :display "r" :text "r"))) :stripped (:mode git-rebase-mode :name "Git Rebase" :read-only t :text "pick a1b2c3d prepare café\nreword b2c3d4e explain λ\nexec printf '界'\n" :point 1 :line 1 :column 0 :modified t :undo disabled)) :cleanup clean)"##
        ]],
    )
}

fn action_commands_auto_advance_movement_and_undo() -> ParityBatchCase {
    ParityBatchCase::value(
        "action_commands_auto_advance_movement_and_undo",
        r####"(git383-test-run
 "actions"
 (lambda (_root)
   (let ((buffer (generate-new-buffer " *git383-actions*")) states)
     (switch-to-buffer buffer)
     (insert "pick 1111111 one\npick 2222222 two\npick 3333333 three\n")
     (git-rebase-mode)
     (buffer-enable-undo)
     (goto-char (point-min))
     (git-rebase-reword)
     (undo-boundary)
     (push (cons 'reword (git383-test-line-state)) states)
     (let ((git-rebase-auto-advance t))
       (git-rebase-edit))
     (undo-boundary)
     (push (cons 'edit-auto (git383-test-line-state)) states)
     (git-rebase-fixup)
     (undo-boundary)
     (push (cons 'fixup (git383-test-line-state)) states)
     (git-rebase-move-line-up)
     (undo-boundary)
     (push (cons 'move-up (git383-test-buffer-state)) states)
     (git-rebase-move-line-down)
     (undo-boundary)
     (push (cons 'move-down (git383-test-buffer-state)) states)
     (git-rebase-kill-line)
     (undo-boundary)
     (push (cons 'kill (git383-test-buffer-state)) states)
     (git-rebase-undo)
     (push (cons 'undo (git383-test-buffer-state)) states)
     (nreverse states))))"####,
        expect![[
            r#"OK (:result ((reword :line 1 :column 0 :text "reword 1111111 one") (edit-auto :line 2 :column 0 :text "pick 2222222 two") (fixup :line 2 :column 0 :text "fixup 2222222 two") (move-up :mode git-rebase-mode :name "Git Rebase" :read-only t :text "fixup 2222222 two\nedit 1111111 one\npick 3333333 three\n" :point 1 :line 1 :column 0 :modified t :undo present) (move-down :mode git-rebase-mode :name "Git Rebase" :read-only t :text "edit 1111111 one\nfixup 2222222 two\npick 3333333 three\n" :point 18 :line 2 :column 0 :modified t :undo present) (kill :mode git-rebase-mode :name "Git Rebase" :read-only t :text "edit 1111111 one\n#fixup 2222222 two\npick 3333333 three\n" :point 37 :line 3 :column 0 :modified t :undo present) (undo :mode git-rebase-mode :name "Git Rebase" :read-only t :text "edit 1111111 one\nfixup 2222222 two\npick 3333333 three\n" :point 18 :line 2 :column 0 :modified t :undo present)) :cleanup clean)"#
        ]],
    )
}

fn exec_command_insertion_editing_and_resurrection() -> ParityBatchCase {
    ParityBatchCase::value(
        "exec_command_insertion_editing_and_resurrection",
        r####"(git383-test-run
 "exec"
 (lambda (_root)
   (let ((buffer (generate-new-buffer " *git383-exec*"))
         (shell-command-history nil) inserted killed revived)
     (switch-to-buffer buffer)
     (insert "pick 1111111 one\npick 2222222 two\n")
     (git-rebase-mode)
     (buffer-enable-undo)
     (goto-char (point-min))
     (setq inserted
           (git383-test-with-inputs '(("" . "printf 'Ω'"))
             (call-interactively #'git-rebase-exec)
             (git383-test-buffer-state)))
     (git-rebase-kill-line)
     (setq killed (git383-test-buffer-state))
     (forward-line -1)
     (git-rebase-exec nil)
     (setq revived (git383-test-buffer-state))
     (list :inserted inserted :killed killed
           :revived revived
           :history shell-command-history))))"####,
        expect![[
            r#"OK (:result (:inserted (:value (:mode git-rebase-mode :name "Git Rebase" :read-only t :text "pick 1111111 one\nexec printf 'Ω'\npick 2222222 two\n" :point 18 :line 2 :column 0 :modified t :undo present) :minibuffers ((:prompt "Execute: " :initial ""))) :killed (:mode git-rebase-mode :name "Git Rebase" :read-only t :text "pick 1111111 one\n#exec printf 'Ω'\npick 2222222 two\n" :point 35 :line 3 :column 0 :modified t :undo present) :revived (:mode git-rebase-mode :name "Git Rebase" :read-only t :text "pick 1111111 one\nexec printf 'Ω'\npick 2222222 two\n" :point 18 :line 2 :column 0 :modified t :undo present) :history ("printf 'Ω'")) :cleanup clean)"#
        ]],
    )
}

fn insert_and_show_commit_boundaries_fail_closed_then_recover() -> ParityBatchCase {
    ParityBatchCase::value(
        "insert_and_show_commit_boundaries_fail_closed_then_recover",
        r####"(git383-test-run
 "boundaries"
 (lambda (root)
   (let* ((log (expand-file-name "git.log" root))
          (tool (git383-test-write-git root))
          (bin (file-name-directory tool))
          (exec-path (list bin))
          (default-directory (file-name-as-directory root))
          (process-environment
           (list (concat "PATH=" bin) (concat "GIT383_LOG=" log)
                 "LC_ALL=C.UTF-8"))
          (buffer (generate-new-buffer " *git383-boundary*"))
          (minibuffer-history nil)
          fallback fallback-state after-failure-undo recovery shown
          integration-ledger)
     (switch-to-buffer buffer)
     (insert "pick 1111111 one\n")
     (git-rebase-mode)
     (buffer-enable-undo)
     (setq buffer-undo-list nil)
     (set-buffer-modified-p nil)
     (goto-char (point-min))
     (setq fallback
           (git383-test-with-inputs '(("" . "topic/界"))
             (condition-case error
                 (progn (call-interactively #'git-rebase-insert)
                        'unexpected-success)
               (error (list (car error) (error-message-string error))))))
     (setq fallback-state (git383-test-buffer-state))
     (when (buffer-modified-p)
       (undo-boundary)
       (git-rebase-undo))
     (setq after-failure-undo (git383-test-buffer-state))
     (setq recovery
           (cl-letf (((symbol-function 'magit-read-branch-or-commit)
                      (lambda (prompt)
                        (push (list :read prompt) integration-ledger)
                        "topic/界"))
                     ((symbol-function 'magit-rev-format)
                      (lambda (format revision)
                        (push (list :format format :revision revision)
                              integration-ledger)
                        "a9be7ac inserted fallback")))
             (call-interactively #'git-rebase-insert)
             (git383-test-buffer-state)))
     (forward-line -1)
     (git-rebase-show-commit)
     (setq shown
           (with-current-buffer "*Shell Command Output*"
             (list :mode major-mode
                   :text (buffer-substring-no-properties
                          (point-min) (point-max)))))
     (list :fallback fallback :failure-state fallback-state
           :after-failure-undo after-failure-undo
           :after-recovery recovery
           :integration (nreverse integration-ledger)
           :shown shown :boundary (git383-test-read-file log)))))"####,
        expect![[
            r#"OK (:result (:fallback (:value (wrong-type-argument "Wrong type argument: char-or-string-p, (\"a9be7ac inserted fallback\")") :minibuffers ((:prompt "Insert revision: " :initial ""))) :failure-state (:mode git-rebase-mode :name "Git Rebase" :read-only t :text "pick 1111111 one\npick " :point 23 :line 2 :column 5 :modified t :undo present) :after-failure-undo (:mode git-rebase-mode :name "Git Rebase" :read-only t :text "pick 1111111 one\n" :point 18 :line 2 :column 0 :modified nil :undo present) :after-recovery (:mode git-rebase-mode :name "Git Rebase" :read-only t :text "pick 1111111 one\npick a9be7ac inserted fallback\n" :point 49 :line 3 :column 0 :modified t :undo present) :integration ((:read "Insert revision") (:format "%h %s" :revision "topic/界")) :shown (:mode fundamental-mode :text "commit a9be7ac870bd756a3fde9041bf9385c23642ec4a\nAuthor: Test User <test@example.invalid>\nDate:   Thu Jan 22 19:14:15 2015 +0000\n\n    inserted fallback\n") :boundary "argv<show><-s><--format=%h %s><topic/\347\225\214>\nargv<show><a9be7ac>\n") :cleanup clean)"#
        ]],
    )
}

fn public_finish_and_abort_drive_real_server_client_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_finish_and_abort_drive_real_server_client_lifecycle",
        r####"(git383-test-run
 "server"
 (lambda (root)
   (let ((server-kill-new-buffers nil)
         (server-log nil)
         (make-backup-files nil)
         finish abort abort-input)
     (let* ((path (expand-file-name "finish/git-rebase-todo" root))
            (_ (make-directory (file-name-directory path) t))
            (_ (write-region "pick 1111111 old\n" nil path nil 'silent))
            (buffer (let ((enable-dir-local-variables nil))
                      (find-file-noselect path)))
            (process (git383-test-make-client "git383-finish" buffer))
            (server-process process)
            (server-clients (list process)))
       (switch-to-buffer buffer)
       (git-rebase-mode)
       (let ((inhibit-read-only t))
         (goto-char (point-max))
         (insert "exec echo finished\n"))
       (git-rebase-server-edit)
       (setq finish (git383-test-server-result buffer process path)))
     (let* ((path (expand-file-name "abort/git-rebase-todo" root))
            (_ (make-directory (file-name-directory path) t))
            (_ (write-region "pick 2222222 abort-me\n" nil path nil 'silent))
            (buffer (let ((enable-dir-local-variables nil))
                      (find-file-noselect path)))
            (process (git383-test-make-client "git383-abort" buffer))
            (server-process process)
            (server-clients (list process)))
       (switch-to-buffer buffer)
       (git-rebase-mode)
       (let ((inhibit-read-only t))
         (goto-char (point-max))
         (insert "pick 3333333 modified\n"))
       (let ((executing-kbd-macro t)
             (unread-command-events (list ?y)))
         (with-timeout (5 (error "git-rebase abort prompt timed out"))
           (git-rebase-abort))
         (setq abort-input (null unread-command-events)))
       (setq abort (git383-test-server-result buffer process path)))
     (list :finish finish :abort abort :abort-input-consumed abort-input))))"####,
        expect![[
            r#"OK (:result (:finish (:buffer-live t :clients nil :modified nil :text "pick 1111111 old\nexec echo finished\n" :file "pick 1111111 old\nexec echo finished\n" :process-live nil :process-member nil) :abort (:buffer-live t :clients nil :modified nil :text "" :file "" :process-live nil :process-member nil) :abort-input-consumed t) :cleanup clean)"#
        ]],
    )
}

#[test]
fn git_rebase_mode_package_batch() {
    let cases = vec![
        mode_activation_fontification_and_instruction_policy(),
        action_commands_auto_advance_movement_and_undo(),
        exec_command_insertion_editing_and_resurrection(),
        insert_and_show_commit_boundaries_fail_closed_then_recover(),
        public_finish_and_abort_drive_real_server_client_lifecycle(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed git-rebase-mode parity test");
    assert_oracle_batch_cases(oracle(), test_name, "git_rebase_mode_parity", &cases);
}
