use expect_test::expect;

use super::ParityBatchCase;

fn status_keys_refresh_and_intent_to_add_a_real_release_file() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (let* ((origin-buffer (current-buffer))
         (buffers-before (buffer-list))
         (root (neomacs-evil-magit-test-project "intent-to-add"))
         (default-directory root)
         status-buffer
         refresh-command
         intent-command)
    (unwind-protect
        (progn
          ;; Keep one realistic untracked release file present when the status
          ;; UI is first built so its section keymap owns point reliably.
          (neomacs-evil-magit-test-write
           (expand-file-name "release plan λ.txt" root)
           "ship=v2\nowner=Zoë\n")
          (setq status-buffer (magit-status-setup-buffer root))
          (switch-to-buffer status-buffer)
          (evil-local-mode 1)
          (evil-change-state evil-magit-state)
          (evil-normalize-keymaps)
          ;; A second file appears only after rendering.  Its presence in the
          ;; refreshed UI and Git result proves `gr' ran the real refresh.
          (neomacs-evil-magit-test-write
           (expand-file-name "late QA notes.txt" root)
           "verify=linux,macos\n")
          (setq refresh-command
                (neomacs-evil-magit-test-call-key "gr" 'magit-refresh))
          (magit-section-show-level-4-all)
          (goto-char (point-min))
          (unless (search-forward "release plan" nil t)
            (error "refreshed status omitted release file: visible=%S git=%S"
                   (neomacs-evil-magit-test-visible-text)
                   (magit-git-lines "status" "--short")))
          (beginning-of-line)
          (setq intent-command
                (neomacs-evil-magit-test-call-key
                 "I" 'evil-magit-stage-untracked-file-with-intent))
          (neomacs-evil-magit-test-await-process)
          (magit-refresh)
          (magit-section-show-level-4-all)
          (list
           :commands (list refresh-command intent-command)
           :mode major-mode
           :state evil-state
           :git-status (magit-git-lines "status" "--short")
           :staged (magit-staged-files)
           :unstaged (magit-unstaged-files)
           :untracked (magit-untracked-files)
           :visible (neomacs-evil-magit-test-visible-text)))
      (neomacs-evil-magit-test-kill-project
       root buffers-before origin-buffer))))
"####;
    let expected = expect![[
        r#"OK (:commands (magit-refresh evil-magit-stage-untracked-file-with-intent) :mode magit-status-mode :state normal :git-status (" A \"release plan λ.txt\"" "?? \"late QA notes.txt\"") :staged ("release plan λ.txt") :unstaged ("release plan λ.txt") :untracked ("late QA notes.txt") :visible "Head:     master baseline\n\nUntracked files (1)\nlate QA notes.txt\n\nUnstaged changes (1)\nmodified   release plan λ.txt\n@@ -0,0 +1,2 @@\n+ship=v2\n+owner=Zoë\n\nStaged changes (1)\nnew file   release plan λ.txt\n\nRecent commits\n<HASH> master baseline")"#
    ]];
    ParityBatchCase::value(
        "status_keys_refresh_and_intent_to_add_a_real_release_file",
        elisp_form,
        expected,
    )
}

fn text_mode_toggle_preserves_status_text_and_returns_to_live_magit() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (let* ((origin-buffer (current-buffer))
         (buffers-before (buffer-list))
         (root (neomacs-evil-magit-test-project "text-mode-round-trip"))
         (default-directory root)
         (evil-magit-last-mode nil)
         status-buffer
         enter-command
         return-command)
    (unwind-protect
        (progn
          (neomacs-evil-magit-test-write
           (expand-file-name "notes for v2.md" root)
           "Review API compatibility.\n")
          (setq status-buffer (magit-status-setup-buffer root))
          (switch-to-buffer status-buffer)
          (magit-section-show-level-4-all)
          (evil-local-mode 1)
          (evil-change-state evil-magit-state)
          (evil-normalize-keymaps)
          (goto-char (point-min))
          (search-forward "notes for v2.md")
          (let ((before-text (neomacs-evil-magit-test-visible-text))
                (before-raw (buffer-string))
                (before-point (point)))
            (setq enter-command
                  (neomacs-evil-magit-test-call-key
                   "C-t" 'evil-magit-toggle-text-mode))
            (let ((text-mode-state
                   (list major-mode
                         evil-magit-toggle-text-minor-mode
                         evil-state
                         (= (point) before-point)
                         (equal before-raw (buffer-string)))))
              (setq return-command
                    (neomacs-evil-magit-test-call-key
                     "C-t" 'evil-magit-toggle-text-mode))
              (magit-section-show-level-4-all)
              (let ((after-text
                     (neomacs-evil-magit-test-visible-text)))
              (list
               :commands (list enter-command return-command)
               :text-state text-mode-state
               :returned
               (list major-mode
                     evil-magit-toggle-text-minor-mode
                     evil-state
                     :before before-text
                     :after after-text)
               :repo-status (magit-git-lines "status" "--short"))))))
      (neomacs-evil-magit-test-kill-project
       root buffers-before origin-buffer))))
"####;
    let expected = expect![[
        r#"OK (:commands (evil-magit-toggle-text-mode evil-magit-toggle-text-mode) :text-state (text-mode t normal t t) :returned (magit-status-mode nil normal :before "Head:     master baseline\n\nUntracked files (1)\nnotes for v2.md\n\nRecent commits\n<HASH> master baseline" :after "Head:     master baseline\n\nUntracked files (1)\nnotes for v2.md\n\nRecent commits\n<HASH> baseline") :repo-status ("?? \"notes for v2.md\""))"#
    ]];
    ParityBatchCase::value(
        "text_mode_toggle_preserves_status_text_and_returns_to_live_magit",
        elisp_form,
        expected,
    )
}

fn double_yank_copies_one_exact_status_line_without_touching_git() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (let* ((origin-buffer (current-buffer))
         (buffers-before (buffer-list))
         (root (neomacs-evil-magit-test-project "status-yank"))
         (default-directory root)
         (kill-ring nil)
         (kill-ring-yank-pointer nil)
         (select-enable-clipboard nil)
         status-buffer
         yank-command)
    (unwind-protect
        (progn
          (neomacs-evil-magit-test-write
           (expand-file-name "alpha plan.txt" root) "first\n")
          (neomacs-evil-magit-test-write
           (expand-file-name "βeta.md" root) "second\n")
          (setq status-buffer (magit-status-setup-buffer root))
          (switch-to-buffer status-buffer)
          (magit-section-show-level-4-all)
          (evil-local-mode 1)
          (evil-change-state evil-magit-state)
          (evil-normalize-keymaps)
          (goto-char (point-min))
          (search-forward "alpha plan.txt")
          (beginning-of-line)
          (setq yank-command (key-binding (kbd "yy")))
          (unless (eq yank-command 'evil-magit-yank-whole-line)
            (error "evil-magit key yy resolved to %S" yank-command))
          (execute-kbd-macro (kbd "yy"))
          (list
           :command yank-command
           :yanked (current-kill 0 t)
           :point-line (buffer-substring-no-properties
                        (line-beginning-position) (line-end-position))
           :git-status (magit-git-lines "status" "--short")
           :files (sort (magit-untracked-files) #'string<)))
      (neomacs-evil-magit-test-kill-project
       root buffers-before origin-buffer))))
"####;
    let expected = expect![[
        r#"OK (:command evil-magit-yank-whole-line :yanked #("alpha plan.txt\n" 0 15 (yank-handler (evil-yank-line-handler nil t))) :point-line "alpha plan.txt" :git-status ("?? \"alpha plan.txt\"" "?? βeta.md") :files ("alpha plan.txt" "βeta.md"))"#
    ]];
    ParityBatchCase::value(
        "double_yank_copies_one_exact_status_line_without_touching_git",
        elisp_form,
        expected,
    )
}

fn rebase_keys_reorder_and_drop_real_todo_entries() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (let ((origin-buffer (current-buffer))
        (buffer (generate-new-buffer "*evil-magit-rebase-plan*"))
        (kill-ring nil)
        (kill-ring-yank-pointer nil)
        down-command
        move-command
        drop-command)
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (insert
           "pick 1111111 prepare release\n"
           "pick 2222222 update docs\n"
           "pick 3333333 publish artifacts\n"
           "\n"
           "# Rebase 0000000..3333333 onto 0000000 (3 commands)\n"
           "#\n"
           "# p, pick <commit> = use commit\n"
           "# r, reword <commit> = use commit, but edit the commit message\n")
          (goto-char (point-min))
          (git-rebase-mode)
          (evil-local-mode 1)
          (evil-change-state evil-magit-state)
          (evil-normalize-keymaps)
          (setq down-command
                (neomacs-evil-magit-test-call-key "j" 'evil-next-line))
          (setq move-command
                (neomacs-evil-magit-test-call-key
                 "M-j" 'git-rebase-move-line-down))
          (setq drop-command
                (neomacs-evil-magit-test-call-key
                 "d" 'git-rebase-kill-line))
          (list
           :commands (list down-command move-command drop-command)
           :mode major-mode
           :state evil-state
           :line (line-number-at-pos)
           :text (buffer-substring-no-properties (point-min) (point-max))))
      (when (buffer-live-p buffer)
        (when (buffer-live-p origin-buffer)
          (switch-to-buffer origin-buffer))
        (with-current-buffer buffer
          ;; `git-rebase-mode' protects a live editor buffer from accidental
          ;; user kills.  This generated test buffer has no backing process,
          ;; so bypass that user prompt only during unconditional cleanup.
          (setq-local kill-buffer-query-functions nil)
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))
"####;
    let expected = expect![[
        r#"OK (:commands (evil-next-line git-rebase-move-line-down git-rebase-kill-line) :mode git-rebase-mode :state normal :line 4 :text "pick 1111111 prepare release\npick 3333333 publish artifacts\n# pick 2222222 update docs\n\n# Rebase 0000000..3333333 onto 0000000 (3 commands)\n#\n# p        pick = use commit\n# r        reword = use commit, but edit the commit message\n# e        edit = use commit, but stop for amending\n# s        squash = use commit, but meld into previous commit\n# f        fixup = like \"squash\", but discard this commit's log message\n# x        exec = run command (the rest of the line) using shell\n# d        drop = remove commit\n# u        undo last change\n# C-c C-c  tell Git to make it happen\n# C-c C-k  tell Git that you changed your mind, i.e. abort\n# k        move point to previous line\n# j        move point to next line\n# M-k      move the commit at point up\n# M-j      move the commit at point down\n# RET      show the commit at point in another buffer\n")"#
    ]];
    ParityBatchCase::value(
        "rebase_keys_reorder_and_drop_real_todo_entries",
        elisp_form,
        expected,
    )
}

fn revert_surfaces_current_magit_transient_incompatibility() -> ParityBatchCase {
    let elisp_form = r####"
(evil-magit-revert)
"####;
    let expected = expect![[r#"ERR (error "' not found in magit-dispatch")"#]];
    ParityBatchCase::signal(
        "revert_surfaces_current_magit_transient_incompatibility",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn text_mode_toggle_rejects_an_unrelated_user_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(let ((evil-magit-last-mode nil))
  (with-temp-buffer
    (text-mode)
    (evil-magit-toggle-text-mode)))
"####;
    let expected = expect![[r#"ERR (user-error "evil-magit-toggle-text-mode unexpected state")"#]];
    ParityBatchCase::signal(
        "text_mode_toggle_rejects_an_unrelated_user_buffer",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        status_keys_refresh_and_intent_to_add_a_real_release_file(),
        text_mode_toggle_preserves_status_text_and_returns_to_live_magit(),
        double_yank_copies_one_exact_status_line_without_touching_git(),
        rebase_keys_reorder_and_drop_real_todo_entries(),
        revert_surfaces_current_magit_transient_incompatibility(),
        text_mode_toggle_rejects_an_unrelated_user_buffer(),
    ]
}
