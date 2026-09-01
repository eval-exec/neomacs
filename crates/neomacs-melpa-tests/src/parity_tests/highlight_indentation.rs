use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HIGHLIGHT_INDENTATION_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HIGHLIGHT_INDENTATION_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HIGHLIGHT_INDENTATION_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'highlight-indentation)

(defun neomacs-highlight-indentation-test-in-buffer (text body)
  "Run BODY in a displayed work buffer containing TEXT."
  (let ((buffer (generate-new-buffer "*highlight-indentation-parity*")))
    (unwind-protect
        (save-window-excursion
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (insert text)
          (goto-char (point-min))
          (funcall body))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-highlight-indentation-test-overlays (property)
  "Return stable summaries for overlays marked with PROPERTY."
  (mapcar
   (lambda (overlay)
     (let ((after (overlay-get overlay 'after-string)))
       (list
        (overlay-start overlay)
        (overlay-end overlay)
        (line-number-at-pos (overlay-start overlay))
        (save-excursion
          (goto-char (overlay-start overlay))
          (current-column))
        (overlay-get overlay 'priority)
        (overlay-get overlay 'face)
        (and after
             (list (substring-no-properties after)
                   (mapcar (lambda (index)
                             (get-text-property index 'face after))
                           (number-sequence 0 (1- (length after)))))))))
   (sort
    (cl-remove-if-not
     (lambda (overlay) (overlay-get overlay property))
     (overlays-in (point-min) (point-max)))
    (lambda (left right)
      (if (= (overlay-start left) (overlay-start right))
          (< (overlay-end left) (overlay-end right))
        (< (overlay-start left) (overlay-start right)))))))

(defun neomacs-highlight-indentation-test-hook-installed-p (hook-spec)
  "Return non-nil when the function in HOOK-SPEC is locally installed."
  (and (local-variable-p (car hook-spec))
       (memq (nth 1 hook-spec) (symbol-value (car hook-spec)))
       t))
"##;

fn highlight_indentation_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HIGHLIGHT_INDENTATION_MELPA_PIN, "highlight-indentation.el")
        .expect("prepare revision-pinned Highlight Indentation source below ./tmp")
        .with_prelude(HIGHLIGHT_INDENTATION_TEST_PRELUDE)
        .with_timeout(HIGHLIGHT_INDENTATION_TEST_TIMEOUT)
}

fn python_workflow_renders_space_guides_with_the_major_mode_offset_and_ignores_tabs()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-highlight-indentation-test-in-buffer
 "def deploy():\n  if ready:\n    ship()\n\tlegacy_tab()\n  audit()\n"
 (lambda ()
   (setq-local major-mode 'python-mode)
   (setq-local python-indent-offset 2)
   (highlight-indentation-mode 1)
   (list
    :mode highlight-indentation-mode
    :offset highlight-indentation-offset
    :lighter (assq 'highlight-indentation-mode minor-mode-alist)
    :hooks
    (mapcar #'neomacs-highlight-indentation-test-hook-installed-p
            highlight-indentation-hooks)
    :guides
    (neomacs-highlight-indentation-test-overlays
     'highlight-indentation-overlay))))
"##;
    let expected = expect![[
        r####"OK (:mode t :offset 2 :lighter (highlight-indentation-mode " ||") :hooks (t t) :guides ((15 16 2 0 1 highlight-indentation-face nil) (27 28 3 0 1 highlight-indentation-face nil) (29 30 3 2 1 highlight-indentation-face nil) (52 53 5 0 1 highlight-indentation-face nil)))"####
    ]];
    ParityBatchCase::value(
        "python_workflow_renders_space_guides_with_the_major_mode_offset_and_ignores_tabs",
        elisp_form,
        expected,
    )
}

fn live_refactoring_updates_affected_guides_and_mode_shutdown_preserves_foreign_overlays()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-highlight-indentation-test-in-buffer
 "pipeline:\n  build\n  test\n    deploy\n"
 (lambda ()
   (setq-local highlight-indentation-offset 2)
   (let ((foreign (make-overlay (point-min) (1+ (point-min))))
         before after-indent after-outdent disabled)
     (overlay-put foreign 'audit-overlay t)
     (highlight-indentation-mode 1)
     (setq before
           (neomacs-highlight-indentation-test-overlays
            'highlight-indentation-overlay))
     (goto-char (point-min))
     (forward-line 2)
     (insert "  ")
     (setq after-indent
           (neomacs-highlight-indentation-test-overlays
            'highlight-indentation-overlay))
     (goto-char (point-min))
     (forward-line 3)
     (delete-char 2)
     (setq after-outdent
           (neomacs-highlight-indentation-test-overlays
            'highlight-indentation-overlay))
     (highlight-indentation-mode -1)
     (setq disabled
           (list
            :mode highlight-indentation-mode
            :guides
            (neomacs-highlight-indentation-test-overlays
             'highlight-indentation-overlay)
            :foreign-live
            (and (overlay-buffer foreign)
                 (buffer-name (overlay-buffer foreign)))
            :hooks
            (mapcar #'neomacs-highlight-indentation-test-hook-installed-p
                    highlight-indentation-hooks)))
     (list :before before
           :after-indent after-indent
           :after-outdent after-outdent
           :text (buffer-string)
           :disabled disabled))))
"##;
    let expected = expect![[
        r####"OK (:before ((11 12 2 0 1 highlight-indentation-face nil) (19 20 3 0 1 highlight-indentation-face nil) (26 27 4 0 1 highlight-indentation-face nil) (28 29 4 2 1 highlight-indentation-face nil)) :after-indent ((11 12 2 0 1 highlight-indentation-face nil) (19 20 3 0 1 highlight-indentation-face nil) (21 22 3 2 1 highlight-indentation-face nil) (28 29 4 0 1 highlight-indentation-face nil) (28 29 4 0 1 highlight-indentation-face nil) (30 31 4 2 1 highlight-indentation-face nil) (30 31 4 2 1 highlight-indentation-face nil)) :after-outdent ((11 12 2 0 1 highlight-indentation-face nil) (19 20 3 0 1 highlight-indentation-face nil) (21 22 3 2 1 highlight-indentation-face nil) (28 29 4 0 1 highlight-indentation-face nil)) :text "pipeline:\n  build\n    test\n  deploy\n" :disabled (:mode nil :guides nil :foreign-live "*highlight-indentation-parity*" :hooks (nil nil)))"####
    ]];
    ParityBatchCase::value(
        "live_refactoring_updates_affected_guides_and_mode_shutdown_preserves_foreign_overlays",
        elisp_form,
        expected,
    )
}

fn blank_line_guides_follow_block_depth_during_edit_and_cleanup_on_disable() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-highlight-indentation-test-in-buffer
 "root\n\n    child\n      grandchild\n  sibling\n"
 (lambda ()
   (setq-local highlight-indentation-offset 2)
   (let ((highlight-indentation-blank-lines t)
         enabled refreshed disabled)
     (highlight-indentation-mode 1)
     (setq enabled
           (neomacs-highlight-indentation-test-overlays
            'highlight-indentation-overlay))
     (goto-char (point-min))
     (forward-line 1)
     (insert " ")
     (delete-char -1)
     (setq refreshed
           (neomacs-highlight-indentation-test-overlays
            'highlight-indentation-overlay))
     (highlight-indentation-mode -1)
     (setq disabled
           (neomacs-highlight-indentation-test-overlays
            'highlight-indentation-overlay))
     (list :enabled enabled
           :refreshed refreshed
           :same-layout-after-space-backspace (equal enabled refreshed)
           :disabled disabled))))
"##;
    let expected = expect![[
        r####"OK (:enabled ((6 6 2 0 1 nil ("    " (highlight-indentation-face nil highlight-indentation-face nil))) (7 8 3 0 1 highlight-indentation-face nil) (9 10 3 2 1 highlight-indentation-face nil) (17 18 4 0 1 highlight-indentation-face nil) (19 20 4 2 1 highlight-indentation-face nil) (21 22 4 4 1 highlight-indentation-face nil) (34 35 5 0 1 highlight-indentation-face nil)) :refreshed ((6 6 2 0 1 nil ("    " (highlight-indentation-face nil highlight-indentation-face nil))) (7 8 3 0 1 highlight-indentation-face nil) (7 8 3 0 1 highlight-indentation-face nil) (7 8 3 0 1 highlight-indentation-face nil) (9 10 3 2 1 highlight-indentation-face nil) (9 10 3 2 1 highlight-indentation-face nil) (9 10 3 2 1 highlight-indentation-face nil) (17 18 4 0 1 highlight-indentation-face nil) (19 20 4 2 1 highlight-indentation-face nil) (21 22 4 4 1 highlight-indentation-face nil) (34 35 5 0 1 highlight-indentation-face nil)) :same-layout-after-space-backspace nil :disabled nil)"####
    ]];
    ParityBatchCase::value(
        "blank_line_guides_follow_block_depth_during_edit_and_cleanup_on_disable",
        elisp_form,
        expected,
    )
}

fn current_column_mode_tracks_navigation_between_nested_blocks_via_post_command_hook()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-highlight-indentation-test-in-buffer
 "root\n  branch\n    leaf\n      detail\n    peer\n  tail\n"
 (lambda ()
   (setq-local highlight-indentation-offset 2)
   (goto-char (point-min))
   (forward-line 1)
   (back-to-indentation)
   (highlight-indentation-current-column-mode 1)
   (let ((branch
          (neomacs-highlight-indentation-test-overlays
           'highlight-indentation-current-column-overlay)))
     (forward-line 1)
     (back-to-indentation)
     (run-hooks 'post-command-hook)
     (let ((leaf
            (neomacs-highlight-indentation-test-overlays
             'highlight-indentation-current-column-overlay)))
       (highlight-indentation-current-column-mode -1)
       (list
        :branch branch
        :leaf leaf
        :point (list (line-number-at-pos) (current-column))
        :hook-after-disable
        (mapcar #'neomacs-highlight-indentation-test-hook-installed-p
                highlight-indentation-current-column-hooks)
        :overlays-after-disable
        (neomacs-highlight-indentation-test-overlays
         'highlight-indentation-current-column-overlay))))))
"##;
    let expected = expect![[
        r####"OK (:branch ((17 18 3 2 2 highlight-indentation-current-column-face nil) (26 27 4 2 2 highlight-indentation-current-column-face nil) (39 40 5 2 2 highlight-indentation-current-column-face nil)) :leaf ((28 29 4 4 2 highlight-indentation-current-column-face nil)) :point (3 4) :hook-after-disable (nil) :overlays-after-disable nil)"####
    ]];
    ParityBatchCase::value(
        "current_column_mode_tracks_navigation_between_nested_blocks_via_post_command_hook",
        elisp_form,
        expected,
    )
}

fn project_configuration_can_change_the_local_guide_width_without_duplicate_hooks()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-highlight-indentation-test-in-buffer
 "job:\n      stage\n        action\n"
 (lambda ()
   (setq-local major-mode 'js-mode)
   (setq-local js-indent-level 2)
   (highlight-indentation-mode 1)
   (let ((guessed highlight-indentation-offset)
         (initial
          (neomacs-highlight-indentation-test-overlays
           'highlight-indentation-overlay)))
     (highlight-indentation-set-offset 4)
     (list
      :guessed guessed
      :configured highlight-indentation-offset
      :local (local-variable-p 'highlight-indentation-offset)
      :initial initial
      :configured-guides
      (neomacs-highlight-indentation-test-overlays
       'highlight-indentation-overlay)
      :hook-counts
      (mapcar
       (lambda (hook-spec)
         (cl-count (nth 1 hook-spec) (symbol-value (car hook-spec))))
       highlight-indentation-hooks)))))
"##;
    let expected = expect![[
        r####"OK (:guessed 2 :configured 4 :local t :initial ((6 7 2 0 1 highlight-indentation-face nil) (8 9 2 2 1 highlight-indentation-face nil) (10 11 2 4 1 highlight-indentation-face nil) (18 19 3 0 1 highlight-indentation-face nil) (20 21 3 2 1 highlight-indentation-face nil) (22 23 3 4 1 highlight-indentation-face nil) (24 25 3 6 1 highlight-indentation-face nil)) :configured-guides ((6 7 2 0 1 highlight-indentation-face nil) (10 11 2 4 1 highlight-indentation-face nil) (18 19 3 0 1 highlight-indentation-face nil) (22 23 3 4 1 highlight-indentation-face nil)) :hook-counts (1 1))"####
    ]];
    ParityBatchCase::value(
        "project_configuration_can_change_the_local_guide_width_without_duplicate_hooks",
        elisp_form,
        expected,
    )
}

#[test]
fn highlight_indentation_package_batch() {
    assert_oracle_batch_cases(
        highlight_indentation_oracle(),
        "highlight-indentation-package-batch",
        "Highlight Indentation",
        &[
            python_workflow_renders_space_guides_with_the_major_mode_offset_and_ignores_tabs(),
            live_refactoring_updates_affected_guides_and_mode_shutdown_preserves_foreign_overlays(),
            blank_line_guides_follow_block_depth_during_edit_and_cleanup_on_disable(),
            current_column_mode_tracks_navigation_between_nested_blocks_via_post_command_hook(),
            project_configuration_can_change_the_local_guide_width_without_duplicate_hooks(),
        ],
    );
}
