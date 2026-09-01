use expect_test::expect;

use super::ParityBatchCase;

fn public_time_highlighting_maps_real_blame_dates_to_configured_eras() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_time_highlighting_maps_real_blame_dates_to_configured_eras",
        r##"
(neomacs-smeargle-test-with-repo
  (let ((smeargle-colors
         '((older-than-1day . "day")
           (older-than-3day . "three-day")
           (older-than-1week . "week")
           (older-than-2week . "two-week")
           (older-than-1month . "month")
           (older-than-3month . "three-month")
           (older-than-6month . "six-month")
           (older-than-1year . "year"))))
    (cl-letf (((symbol-function 'current-time)
               (lambda () (date-to-time "2026-08-07T12:00:00+0000"))))
      (smeargle)
      (neomacs-smeargle-test-wait file))
    (list :repo (smeargle--repo-type)
          :command (smeargle--blame-command 'git)
          :overlays (neomacs-smeargle-test-overlays))))
"##,
        expect![[
            r#"OK (:repo git :command ("git" "--no-pager" "blame" "[ORACLE-SANDBOX]/smeargle-repo/release.txt") :overlays ((:start-line 1 :end-line 1 :text "ancient\n" :face (:background "year" . #1=(:extend t))) (:start-line 2 :end-line 2 :text "six-month updated\n" :face (:background "six-month" . #1#)) (:start-line 3 :end-line 3 :text "month updated\n" :face (:background "month" . #1#)) (:start-line 4 :end-line 4 :text "weeks updated\n" :face (:background "two-week" . #1#)) (:start-line 5 :end-line 5 :text "days updated\n" :face (:background "three-day" . #1#)) (:start-line 6 :end-line 6 :text "yesterday updated\n" :face (:background "day" . #1#))))"#
        ]],
    )
}

fn public_commit_age_highlighting_ranks_newest_changes_and_caps_old_ages() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_commit_age_highlighting_ranks_newest_changes_and_caps_old_ages",
        r##"
(neomacs-smeargle-test-with-repo
  (let ((smeargle-age-threshold 3)
        (smeargle-age-colors
         '((0 . "age-0") (1 . "age-1") (2 . "age-2") (3 . "age-3"))))
    (smeargle-commits)
    (neomacs-smeargle-test-wait file)
    (list :overlays (neomacs-smeargle-test-overlays)
          :count
          (length (seq-filter
                   (lambda (overlay) (overlay-get overlay 'smeargle))
                   (overlays-in (point-min) (point-max)))))))
"##,
        expect![[
            r#"OK (:overlays ((:start-line 1 :end-line 1 :text "ancient\n" :face (:background "age-3" . #1=(:extend t))) (:start-line 2 :end-line 2 :text "six-month updated\n" :face (:background "age-3" . #1#)) (:start-line 3 :end-line 3 :text "month updated\n" :face (:background "age-3" . #1#)) (:start-line 4 :end-line 4 :text "weeks updated\n" :face (:background "age-2" . #1#)) (:start-line 5 :end-line 5 :text "days updated\n" :face (:background "age-1" . #1#)) (:start-line 6 :end-line 6 :text "yesterday updated\n" :face (:background "age-0" . #1#))) :count 6)"#
        ]],
    )
}

fn rerunning_replaces_old_overlays_with_the_new_color_policy() -> ParityBatchCase {
    ParityBatchCase::value(
        "rerunning_replaces_old_overlays_with_the_new_color_policy",
        r##"
(neomacs-smeargle-test-with-repo
  (let ((smeargle-colors
         '((older-than-1day . "old-day")
           (older-than-3day . "old-three")
           (older-than-1week . "old-week")
           (older-than-2week . "old-two-week")
           (older-than-1month . "old-month")
           (older-than-3month . "old-three-month")
           (older-than-6month . "old-six-month")
           (older-than-1year . "old-year"))))
    (cl-letf (((symbol-function 'current-time)
               (lambda () (date-to-time "2026-08-07T12:00:00+0000"))))
      (smeargle)
      (neomacs-smeargle-test-wait file)
      (let ((first (neomacs-smeargle-test-overlays)))
        (setq smeargle-colors
              (mapcar (lambda (entry)
                        (cons (car entry) (concat "new-" (cdr entry))))
                      smeargle-colors))
        (smeargle)
        (neomacs-smeargle-test-wait file)
        (list :first first
              :second (neomacs-smeargle-test-overlays)
              :old-colors-left
              (seq-some
               (lambda (overlay)
                 (string-prefix-p
                  "old-"
                  (plist-get (overlay-get overlay 'face) :background)))
               (seq-filter
                (lambda (overlay) (overlay-get overlay 'smeargle))
                (overlays-in (point-min) (point-max)))))))))
"##,
        expect![[
            r#"OK (:first ((:start-line 1 :end-line 1 :text "ancient\n" :face (:background "old-year" . #1=(:extend t))) (:start-line 2 :end-line 2 :text "six-month updated\n" :face (:background "old-six-month" . #1#)) (:start-line 3 :end-line 3 :text "month updated\n" :face (:background "old-month" . #1#)) (:start-line 4 :end-line 4 :text "weeks updated\n" :face (:background "old-two-week" . #1#)) (:start-line 5 :end-line 5 :text "days updated\n" :face (:background "old-three" . #1#)) (:start-line 6 :end-line 6 :text "yesterday updated\n" :face (:background "old-day" . #1#))) :second ((:start-line 1 :end-line 1 :text "ancient\n" :face (:background "new-old-year" . #1#)) (:start-line 2 :end-line 2 :text "six-month updated\n" :face (:background "new-old-six-month" . #1#)) (:start-line 3 :end-line 3 :text "month updated\n" :face (:background "new-old-month" . #1#)) (:start-line 4 :end-line 4 :text "weeks updated\n" :face (:background "new-old-two-week" . #1#)) (:start-line 5 :end-line 5 :text "days updated\n" :face (:background "new-old-three" . #1#)) (:start-line 6 :end-line 6 :text "yesterday updated\n" :face (:background "new-old-day" . #1#))) :old-colors-left nil)"#
        ]],
    )
}

fn clear_removes_only_smeargle_overlays_and_keeps_unrelated_annotations() -> ParityBatchCase {
    ParityBatchCase::value(
        "clear_removes_only_smeargle_overlays_and_keeps_unrelated_annotations",
        r##"
(neomacs-smeargle-test-with-repo
  (let ((unrelated (make-overlay (point-min) (line-end-position)))
        (smeargle-age-colors
         '((0 . "age-0") (1 . "age-1") (2 . "age-2")
           (3 . "age-3") (4 . "age-4") (5 . "age-5"))))
    (overlay-put unrelated 'category 'unrelated)
    (smeargle-commits)
    (neomacs-smeargle-test-wait file)
    (let ((before (length (neomacs-smeargle-test-overlays))))
      (smeargle-clear)
      (list :before before
            :after (neomacs-smeargle-test-overlays)
            :unrelated-live (and (overlay-buffer unrelated) t)
            :unrelated-category (overlay-get unrelated 'category)))))
"##,
        expect!["OK (:before 6 :after nil :unrelated-live t :unrelated-category unrelated)"],
    )
}

fn public_command_rejects_files_outside_supported_repositories() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_command_rejects_files_outside_supported_repositories",
        r##"
(let* ((root (file-name-as-directory
              (expand-file-name "not-a-repo"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (file (expand-file-name "plain.txt" root)))
  (make-directory root t)
  (with-temp-file file (insert "plain\n"))
  (let ((buffer (find-file-noselect file)))
    (unwind-protect
        (with-current-buffer buffer
          ;; All permitted scratch paths live below the repository's ./tmp
          ;; tree, so `locate-dominating-file' would otherwise discover the
          ;; parent Neomacs checkout.  Substitute only that filesystem
          ;; boundary while keeping Smeargle's public command and detection
          ;; logic real.
          (cl-letf (((symbol-function 'locate-dominating-file)
                     (lambda (&rest _) nil)))
            (list :repo (smeargle--repo-type)
                  :outcome
                  (condition-case err
                      (list :value (smeargle))
                    (error
                     (list :signal (car err)
                           :message (error-message-string err)))))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer))
      (delete-directory root t))))
"##,
        expect![[
            r#"OK (:repo nil :outcome (:signal user-error :message "Here is not ’git’ or ’mercurial’ repository"))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        public_time_highlighting_maps_real_blame_dates_to_configured_eras(),
        public_commit_age_highlighting_ranks_newest_changes_and_caps_old_ages(),
        rerunning_replaces_old_overlays_with_the_new_color_policy(),
        clear_removes_only_smeargle_overlays_and_keeps_unrelated_annotations(),
        public_command_rejects_files_outside_supported_repositories(),
    ]
}
