use expect_test::expect;

use super::ParityBatchCase;

fn staging_a_real_release_change_schedules_and_applies_one_treemacs_refresh() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-treemacs-magit-test-with-project "stage-refresh" 'simple
  (let* ((release-file (expand-file-name "release.txt" root))
         (idle-before (copy-sequence timer-idle-list)))
    (neomacs-treemacs-magit-test-write
     release-file "version=2\nchannel=canary\n")
    (let ((this-command 'magit-stage-files))
      (magit-stage-files (list "release.txt")))
    (let* ((new-idle
            (neomacs-treemacs-magit-test-new-timers
             timer-idle-list idle-before))
           (scheduled-root (car treemacs-magit--timers))
           (before-dispatch
            (list :git-status (magit-git-lines "status" "--short")
                  :staged (magit-staged-files)
                  :scheduled-roots
                  (mapcar (lambda (path) (file-relative-name path root))
                          (copy-sequence treemacs-magit--timers))
                  :new-idle-count (length new-idle)
                  :idle-seconds (and (= (length new-idle) 1)
                                     (float-time
                                      (timer--time (car new-idle))))
                  :refresh-flags
                  (neomacs-treemacs-magit-test-refresh-flags
                   treemacs-buffer root))))
      (unless (and (= (length new-idle) 1) scheduled-root)
        (error "stage did not create exactly one Treemacs update: %S / %S"
               new-idle treemacs-magit--timers))
      (timer-event-handler (car new-idle))
      (list
       :before-dispatch before-dispatch
       :after-idle-dispatch
       (list :scheduled-roots
             (mapcar (lambda (path) (file-relative-name path root))
                     (copy-sequence treemacs-magit--timers))
             :filewatch
             (neomacs-treemacs-magit-test-complete-filewatch-refresh
              treemacs-buffer root)
             :git-status (magit-git-lines "status" "--short")
             :index-content (magit-git-lines "show" ":release.txt"))))))
"####;
    let expected = expect![[
        r#"OK (:before-dispatch (:git-status ("M  release.txt") :staged ("release.txt") :scheduled-roots (".") :new-idle-count 1 :idle-seconds 3.0 :refresh-flags nil) :after-idle-dispatch (:scheduled-roots nil :filewatch (:queued (:refresh-flags (("." . force-refresh)) :timer-created t :timer-active t) :completed (:refresh-flags nil :timer-cleared t :timer-active nil)) :git-status ("M  release.txt") :index-content ("version=2" "channel=canary")))"#
    ]];
    ParityBatchCase::value(
        "staging_a_real_release_change_schedules_and_applies_one_treemacs_refresh",
        elisp_form,
        expected,
    )
}

fn rapid_stage_and_unstage_coalesce_before_refreshing_the_final_worktree_state() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-treemacs-magit-test-with-project "coalesced-stage-unstage" 'simple
  (let* ((release-file (expand-file-name "release.txt" root))
         (idle-before (copy-sequence timer-idle-list)))
    (neomacs-treemacs-magit-test-write
     release-file "version=2\nchannel=preview\n")
    (let ((this-command 'magit-stage-files))
      (magit-stage-files (list "release.txt")))
    (let* ((after-stage (copy-sequence timer-idle-list))
           (stage-new
            (neomacs-treemacs-magit-test-new-timers
             after-stage idle-before)))
      (let ((this-command 'magit-unstage-files))
        (magit-unstage-files (list "release.txt")))
      (let ((unstage-new
             (neomacs-treemacs-magit-test-new-timers
              timer-idle-list after-stage)))
        (unless (= (length stage-new) 1)
          (error "stage did not create one idle update: %S" stage-new))
        (let ((before-dispatch
               (list
                :stage-new-idle (length stage-new)
                :unstage-new-idle (length unstage-new)
                :scheduled-roots
                (mapcar (lambda (path) (file-relative-name path root))
                        (copy-sequence treemacs-magit--timers))
                :git-status (magit-git-lines "status" "--short")
                :staged (magit-staged-files)
                :unstaged (magit-unstaged-files)
                :index-content (magit-git-lines "show" ":release.txt"))))
          (timer-event-handler (car stage-new))
          (list
           :before-dispatch before-dispatch
           :after-idle-dispatch
           (list
            :scheduled-roots
            (mapcar (lambda (path) (file-relative-name path root))
                    (copy-sequence treemacs-magit--timers))
            :filewatch
            (neomacs-treemacs-magit-test-complete-filewatch-refresh
             treemacs-buffer root)
            :worktree
            (with-temp-buffer
              (insert-file-contents release-file)
              (buffer-string)))))))))
"####;
    let expected = expect![[
        r#"OK (:before-dispatch (:stage-new-idle 1 :unstage-new-idle 0 :scheduled-roots (".") :git-status (" M release.txt") :staged nil :unstaged ("release.txt") :index-content ("version=1" "channel=stable")) :after-idle-dispatch (:scheduled-roots nil :filewatch (:queued (:refresh-flags (("." . force-refresh)) :timer-created t :timer-active t) :completed (:refresh-flags nil :timer-cleared t :timer-active nil)) :worktree "version=2\nchannel=preview\n"))"#
    ]];
    ParityBatchCase::value(
        "rapid_stage_and_unstage_coalesce_before_refreshing_the_final_worktree_state",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn disabling_treemacs_git_mode_leaves_magit_staging_independent() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-treemacs-magit-test-with-project "git-mode-disabled" nil
  (let* ((release-file (expand-file-name "release.txt" root))
         (idle-before (copy-sequence timer-idle-list)))
    (neomacs-treemacs-magit-test-write
     release-file "version=3\nchannel=stable\n")
    (let ((this-command 'magit-stage-files))
      (magit-stage-files (list "release.txt")))
    (list
     :git-mode (list treemacs-git-mode treemacs--git-mode)
     :git-status (magit-git-lines "status" "--short")
     :staged (magit-staged-files)
     :index-content (magit-git-lines "show" ":release.txt")
     :scheduled-roots (copy-sequence treemacs-magit--timers)
     :new-idle-timers
     (length
      (neomacs-treemacs-magit-test-new-timers
       timer-idle-list idle-before))
     :refresh-flags
     (neomacs-treemacs-magit-test-refresh-flags treemacs-buffer root))))
"####;
    let expected = expect![[
        r#"OK (:git-mode (nil nil) :git-status ("M  release.txt") :staged ("release.txt") :index-content ("version=3" "channel=stable") :scheduled-roots nil :new-idle-timers 0 :refresh-flags nil)"#
    ]];
    ParityBatchCase::value(
        "disabling_treemacs_git_mode_leaves_magit_staging_independent",
        elisp_form,
        expected,
    )
}

fn extending_a_real_commit_schedules_the_same_project_refresh() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-treemacs-magit-test-with-project "commit-refresh" 'simple
  (let* ((release-file (expand-file-name "release.txt" root))
         (idle-before (copy-sequence timer-idle-list)))
    (neomacs-treemacs-magit-test-write
     release-file "version=4\nchannel=production\n")
    ;; Prepare the index without a staging command so this workflow observes
    ;; the post-commit integration independently of the post-stage hook.
    (magit-git "add" "--" "release.txt")
    (let (process)
      (let ((this-command nil)
            (last-command 'magit-commit-extend))
        (setq process (magit-commit-extend nil t))
        (neomacs-treemacs-magit-test-await-process process))
      (let ((new-idle
             (neomacs-treemacs-magit-test-await-idle-timers idle-before)))
        (unless (= (length new-idle) 1)
          (error "commit did not create one idle update: %S" new-idle))
        (let ((before-dispatch
               (list
                :process (list (process-status process)
                               (process-exit-status process))
                :head-count (magit-git-string "rev-list" "--count" "HEAD")
                :subject (magit-git-string "log" "-1" "--format=%s")
                :git-status (magit-git-lines "status" "--short")
                :head-content
                (magit-git-lines "show" "HEAD:release.txt")
                :scheduled-roots
                (mapcar (lambda (path) (file-relative-name path root))
                        (copy-sequence treemacs-magit--timers)))))
          (timer-event-handler (car new-idle))
          (list
           :before-dispatch before-dispatch
           :after-idle-dispatch
           (list
            :scheduled-roots (copy-sequence treemacs-magit--timers)
            :filewatch
            (neomacs-treemacs-magit-test-complete-filewatch-refresh
             treemacs-buffer root))))))))
"####;
    let expected = expect![[
        r#"OK (:before-dispatch (:process (exit 0) :head-count "1" :subject "baseline" :git-status nil :head-content ("version=4" "channel=production") :scheduled-roots (".")) :after-idle-dispatch (:scheduled-roots nil :filewatch (:queued (:refresh-flags (("." . force-refresh)) :timer-created t :timer-active t) :completed (:refresh-flags nil :timer-cleared t :timer-active nil))))"#
    ]];
    ParityBatchCase::value(
        "extending_a_real_commit_schedules_the_same_project_refresh",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn extended_git_mode_recolors_the_visible_file_after_staging() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-treemacs-magit-test-with-project "extended-face-refresh" 'extended
  (let* ((release-file (expand-file-name "release.txt" root))
         (idle-before (copy-sequence timer-idle-list))
         (before-node
          (neomacs-treemacs-magit-test-node-state
           treemacs-buffer release-file root)))
    (neomacs-treemacs-magit-test-write
     release-file "version=5\nchannel=edge\n")
    (let ((this-command 'magit-stage-files))
      (magit-stage-files (list "release.txt")))
    (let ((new-idle
           (neomacs-treemacs-magit-test-new-timers
            timer-idle-list idle-before)))
      (unless (= (length new-idle) 1)
        (error "extended stage did not create one idle update: %S" new-idle))
      (let ((processes-before-update (process-list)))
        (timer-event-handler (car new-idle))
        (neomacs-treemacs-magit-test-await-processes
         processes-before-update))
      (list
       :before-node before-node
       :after-node
       (neomacs-treemacs-magit-test-node-state
        treemacs-buffer release-file root)
       :git-status (magit-git-lines "status" "--short")
       :staged (magit-staged-files)
       :scheduled-roots (copy-sequence treemacs-magit--timers)
       :refresh-flags
       (neomacs-treemacs-magit-test-refresh-flags
        treemacs-buffer root)
       :filewatch-timer-created (timerp treemacs--refresh-timer)))))
"####;
    let expected = expect![[
        r#"OK (:before-node (:path "release.txt" :label "release.txt" :state file-node-closed :face treemacs-git-unmodified-face) :after-node (:path "release.txt" :label "release.txt" :state file-node-closed :face treemacs-git-modified-face) :git-status ("M  release.txt") :staged ("release.txt") :scheduled-roots nil :refresh-flags nil :filewatch-timer-created nil)"#
    ]];
    ParityBatchCase::value(
        "extended_git_mode_recolors_the_visible_file_after_staging",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn staging_an_unregistered_repository_does_not_refresh_the_visible_project() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-treemacs-magit-test-with-project "visible-project" 'simple
  (let* ((visible-root root)
         (outside-root (neomacs-treemacs-magit-test-project "outside-repository"))
         (outside-file (expand-file-name "release.txt" outside-root))
         (default-directory outside-root)
         (idle-before (copy-sequence timer-idle-list)))
    (unwind-protect
        (progn
          (neomacs-treemacs-magit-test-write
           outside-file "version=7\nchannel=private\n")
          (let ((this-command 'magit-stage-files))
            (magit-stage-files (list "release.txt")))
          (let ((new-idle
                 (neomacs-treemacs-magit-test-new-timers
                  timer-idle-list idle-before)))
            (unless (= (length new-idle) 1)
              (error "unregistered stage did not create one idle update: %S"
                     new-idle))
            (let ((before-dispatch
                   (list
                    :git-status (magit-git-lines "status" "--short")
                    :staged (magit-staged-files)
                    :scheduled-roots
                    (mapcar
                     (lambda (path)
                       (file-relative-name
                        path neomacs-treemacs-magit-test-root))
                     (copy-sequence treemacs-magit--timers))
                    :visible-refresh-flags
                    (neomacs-treemacs-magit-test-refresh-flags
                     treemacs-buffer visible-root))))
              (timer-event-handler (car new-idle))
              (list
               :before-dispatch before-dispatch
               :after-dispatch
               (list
                :scheduled-roots (copy-sequence treemacs-magit--timers)
                :visible-refresh-flags
                (neomacs-treemacs-magit-test-refresh-flags
                 treemacs-buffer visible-root)
                :filewatch-timer-created (timerp treemacs--refresh-timer)
                :index-content
                (magit-git-lines "show" ":release.txt"))))))
      (when (file-directory-p outside-root)
        (delete-directory outside-root t)))))
"####;
    let expected = expect![[
        r#"OK (:before-dispatch (:git-status ("M  release.txt") :staged ("release.txt") :scheduled-roots ("outside-repository") :visible-refresh-flags nil) :after-dispatch (:scheduled-roots nil :visible-refresh-flags nil :filewatch-timer-created nil :index-content ("version=7" "channel=private")))"#
    ]];
    ParityBatchCase::value(
        "staging_an_unregistered_repository_does_not_refresh_the_visible_project",
        elisp_form,
        expected,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        staging_a_real_release_change_schedules_and_applies_one_treemacs_refresh(),
        rapid_stage_and_unstage_coalesce_before_refreshing_the_final_worktree_state(),
        disabling_treemacs_git_mode_leaves_magit_staging_independent(),
        extending_a_real_commit_schedules_the_same_project_refresh(),
        extended_git_mode_recolors_the_visible_file_after_staging(),
        staging_an_unregistered_repository_does_not_refresh_the_visible_project(),
    ]
}
