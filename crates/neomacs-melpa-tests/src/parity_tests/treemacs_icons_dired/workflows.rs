use expect_test::expect;

use super::ParityBatchCase;

fn global_mode_updates_existing_and_future_dired_buffers_then_tears_down() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-treemacs-icons-dired-test-root "icons-lifecycle"))
       (existing-root (expand-file-name "existing" root))
       (future-root (expand-file-name "future" root))
       (readme (expand-file-name "README.md" existing-root))
       (source-dir (expand-file-name "src" existing-root))
       (plan (expand-file-name "plan.el" future-root))
       existing future)
  (unwind-protect
      (progn
        (make-directory source-dir t)
        (make-directory future-root t)
        (with-temp-file readme (insert "# Release\n"))
        (with-temp-file plan (insert "(provide 'plan)\n"))
        (setq existing (dired-noselect existing-root "-al"))
        (with-current-buffer existing
          (setq-local tab-width 8))
        (let ((before
               (with-current-buffer existing
                 (list :entry
                       (neomacs-treemacs-icons-dired-test-entry readme)
                       :tab-width tab-width
                       :displayed treemacs-icons-dired-displayed))))
          (cl-letf (((symbol-function 'display-graphic-p)
                     (lambda (&optional _frame) t)))
            (treemacs-icons-dired-mode 1)
            (setq future (dired-noselect future-root "-al"))
            (let ((enabled
                   (list
                    :mode treemacs-icons-dired-mode
                    :registration
                    (neomacs-treemacs-icons-dired-test-registration)
                    :existing
                    (with-current-buffer existing
                      (list
                       :file (neomacs-treemacs-icons-dired-test-entry readme)
                       :directory
                       (neomacs-treemacs-icons-dired-test-entry source-dir)
                       :tab-width tab-width
                       :displayed treemacs-icons-dired-displayed
                       :coverage
                       (neomacs-treemacs-icons-dired-test-coverage existing-root)))
                    :future
                    (with-current-buffer future
                      (list
                       :file (neomacs-treemacs-icons-dired-test-entry plan)
                       :tab-width tab-width
                       :displayed treemacs-icons-dired-displayed
                       :coverage
                       (neomacs-treemacs-icons-dired-test-coverage future-root))))))
              (treemacs-icons-dired-mode -1)
              (list
               :before before
               :enabled enabled
               :disabled
               (list
                :mode treemacs-icons-dired-mode
                :registration
                (neomacs-treemacs-icons-dired-test-registration)
                :existing
                (with-current-buffer existing
                  (list
                   :entry (neomacs-treemacs-icons-dired-test-entry readme)
                   :tab-width tab-width
                   :displayed treemacs-icons-dired-displayed))
                :future
                (with-current-buffer future
                  (list
                   :entry (neomacs-treemacs-icons-dired-test-entry plan)
                   :tab-width tab-width
                   :displayed treemacs-icons-dired-displayed))))))))
    (when treemacs-icons-dired-mode
      (treemacs-icons-dired-mode -1))
    (dolist (buffer (list existing future))
      (when (buffer-live-p buffer) (kill-buffer buffer)))
    (when (file-exists-p root) (delete-directory root t))))
"##;
    let expected = expect![[
        r#"OK (:before (:entry (:name "README.md" :present t :icon nil) :tab-width 8 :displayed nil) :enabled (:mode t :registration (:after-readin t :mode-select t :tab-width t :revert t :add-entry t) :existing (:file (:name "README.md" :present t :icon "[MD]") :directory (:name "src" :present t :icon "[SRC]") :tab-width 1 :displayed t :coverage ("./")) :future (:file (:name "plan.el" :present t :icon "[EL]") :tab-width 1 :displayed t :coverage ("./"))) :disabled (:mode nil :registration (:after-readin nil :mode-select nil :tab-width nil :revert nil :add-entry nil) :existing (:entry (:name "README.md" :present t :icon nil) :tab-width 8 :displayed t) :future (:entry (:name "plan.el" :present t :icon nil) :tab-width 8 :displayed t)))"#
    ]];
    ParityBatchCase::value(
        "global_mode_updates_existing_and_future_dired_buffers_then_tears_down",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn subdirectory_refresh_kill_and_reinsert_follow_the_coverage_registry() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-treemacs-icons-dired-test-root "icons-subdirs"))
       (source-dir (file-name-as-directory (expand-file-name "src" root)))
       (module (expand-file-name "module.el" source-dir))
       buffer)
  (unwind-protect
      (progn
        (make-directory source-dir t)
        (with-temp-file module (insert "(provide 'module)\n"))
        (cl-letf (((symbol-function 'display-graphic-p)
                   (lambda (&optional _frame) t)))
          (treemacs-icons-dired-mode 1)
          (setq buffer (dired-noselect root "-al"))
          (with-current-buffer buffer
            (dired-insert-subdir source-dir)
            (let ((first
                   (list
                    :entry (neomacs-treemacs-icons-dired-test-entry module)
                    :coverage
                    (neomacs-treemacs-icons-dired-test-coverage root))))
              (dired-insert-subdir source-dir)
              (let ((refreshed
                     (list
                      :entry (neomacs-treemacs-icons-dired-test-entry module)
                      :coverage
                      (neomacs-treemacs-icons-dired-test-coverage root))))
                (dired-goto-subdir source-dir)
                (dired-kill-subdir)
                (let ((killed
                       (list
                        :listed (and (assoc source-dir dired-subdir-alist) t)
                        :coverage
                        (neomacs-treemacs-icons-dired-test-coverage root))))
                  (dired-insert-subdir source-dir)
                  (list
                   :first first
                   :refreshed refreshed
                   :killed killed
                   :reinserted
                   (list
                    :entry (neomacs-treemacs-icons-dired-test-entry module)
                    :coverage
                    (neomacs-treemacs-icons-dired-test-coverage root)))))))))
    (when treemacs-icons-dired-mode
      (treemacs-icons-dired-mode -1))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (when (file-exists-p root) (delete-directory root t))))
"##;
    let expected = expect![[
        r#"OK (:first (:entry (:name "module.el" :present t :icon "[EL]") :coverage ("src/" "./")) :refreshed (:entry (:name "module.el" :present t :icon nil) :coverage ("src/" "./")) :killed (:listed nil :coverage ("./")) :reinserted (:entry (:name "module.el" :present t :icon "[EL]") :coverage ("src/" "./")))"#
    ]];
    ParityBatchCase::value(
        "subdirectory_refresh_kill_and_reinsert_follow_the_coverage_registry",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn newly_created_entries_and_full_reverts_keep_icons_in_sync() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-treemacs-icons-dired-test-root "icons-refresh"))
       (readme (expand-file-name "README.md" root))
       (notes (expand-file-name "release-notes.txt" root))
       buffer)
  (unwind-protect
      (progn
        (with-temp-file readme (insert "# Release\n"))
        (cl-letf (((symbol-function 'display-graphic-p)
                   (lambda (&optional _frame) t)))
          (treemacs-icons-dired-mode 1)
          (setq buffer (dired-noselect root "-al"))
          (with-temp-file notes (insert "Shipped\n"))
          (dired-add-file notes)
          (with-current-buffer buffer
            (let ((added
                   (list
                    :readme (neomacs-treemacs-icons-dired-test-entry readme)
                    :notes (neomacs-treemacs-icons-dired-test-entry notes)
                    :displayed treemacs-icons-dired-displayed
                    :coverage
                    (neomacs-treemacs-icons-dired-test-coverage root))))
              (dired-revert)
              (list
               :added added
               :reverted
               (list
                :readme (neomacs-treemacs-icons-dired-test-entry readme)
                :notes (neomacs-treemacs-icons-dired-test-entry notes)
                :displayed treemacs-icons-dired-displayed
                :coverage
                (neomacs-treemacs-icons-dired-test-coverage root)))))))
    (when treemacs-icons-dired-mode
      (treemacs-icons-dired-mode -1))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (when (file-exists-p root) (delete-directory root t))))
"##;
    let expected = expect![[
        r#"OK (:added (:readme (:name "README.md" :present t :icon "[MD]") :notes (:name "release-notes.txt" :present t :icon "[TXT]") :displayed t :coverage ("./")) :reverted (:readme (:name "README.md" :present t :icon "[MD]") :notes (:name "release-notes.txt" :present t :icon "[TXT]") :displayed t :coverage ("./")))"#
    ]];
    ParityBatchCase::value(
        "newly_created_entries_and_full_reverts_keep_icons_in_sync",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn enable_once_respects_the_terminal_gate_then_draws_on_graphical_revert() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (neomacs-treemacs-icons-dired-test-root "icons-enable-once"))
       (notes (expand-file-name "notes.txt" root))
       buffer)
  (unwind-protect
      (progn
        (with-temp-file notes (insert "Ready\n"))
        (add-hook 'dired-mode-hook #'treemacs-icons-dired-enable-once)
        (setq buffer (dired-noselect root "-al"))
        (with-current-buffer buffer
          (let ((terminal
                 (list
                  :mode treemacs-icons-dired-mode
                  :enable-once-hook
                  (and (memq #'treemacs-icons-dired-enable-once
                             dired-mode-hook)
                       t)
                  :entry (neomacs-treemacs-icons-dired-test-entry notes)
                  :tab-width tab-width
                  :displayed treemacs-icons-dired-displayed)))
            (cl-letf (((symbol-function 'display-graphic-p)
                       (lambda (&optional _frame) t)))
              (dired-revert)
              (list
               :terminal terminal
               :graphical
               (list
                :mode treemacs-icons-dired-mode
                :entry (neomacs-treemacs-icons-dired-test-entry notes)
                :tab-width tab-width
                :displayed treemacs-icons-dired-displayed
                :coverage
                (neomacs-treemacs-icons-dired-test-coverage root)))))))
    (remove-hook 'dired-mode-hook #'treemacs-icons-dired-enable-once)
    (when treemacs-icons-dired-mode
      (treemacs-icons-dired-mode -1))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (when (file-exists-p root) (delete-directory root t))))
"##;
    let expected = expect![[
        r#"OK (:terminal (:mode t :enable-once-hook nil :entry (:name "notes.txt" :present t :icon nil) :tab-width 1 :displayed nil) :graphical (:mode t :entry (:name "notes.txt" :present t :icon "[TXT]") :tab-width 8 :displayed t :coverage ("./")))"#
    ]];
    ParityBatchCase::value(
        "enable_once_respects_the_terminal_gate_then_draws_on_graphical_revert",
        elisp_form,
        expected,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        global_mode_updates_existing_and_future_dired_buffers_then_tears_down(),
        subdirectory_refresh_kill_and_reinsert_follow_the_coverage_registry(),
        newly_created_entries_and_full_reverts_keep_icons_in_sync(),
        enable_once_respects_the_terminal_gate_then_draws_on_graphical_revert(),
    ]
}
