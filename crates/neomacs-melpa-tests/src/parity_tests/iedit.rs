use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, IEDIT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const IEDIT_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const IEDIT_TEST_PRELUDE: &str = r##"
(require 'iedit)
(require 'iedit-rect)

(defun neomacs-iedit-test-overlay-snapshot ()
  "Return occurrence overlays in buffer order with their visible state."
  (mapcar
   (lambda (overlay)
     (save-excursion
       (goto-char (overlay-start overlay))
       (list :range (list (overlay-start overlay) (overlay-end overlay))
             :column (current-column)
             :text (buffer-substring-no-properties
                    (overlay-start overlay) (overlay-end overlay))
             :category (overlay-get overlay 'category))))
   (sort (copy-sequence iedit-occurrences-overlays)
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun neomacs-iedit-test-hidden-ranges ()
  "Return Iedit's invisible overlay ranges in buffer order."
  (mapcar
   (lambda (overlay)
     (list (overlay-start overlay) (overlay-end overlay)
           (buffer-substring-no-properties
            (overlay-start overlay) (overlay-end overlay))))
   (sort
    (seq-filter
     (lambda (overlay)
       (overlay-get overlay 'iedit-invisible-overlay-name))
     (overlays-in (point-min) (point-max)))
    (lambda (left right)
      (< (overlay-start left) (overlay-start right))))))

(defun neomacs-iedit-test-run (text body)
  "Run BODY in a displayed temporary buffer initialized with TEXT."
  (save-window-excursion
    (with-temp-buffer
      (set-window-buffer (selected-window) (current-buffer))
      (transient-mark-mode 1)
      (buffer-enable-undo)
      (insert text)
      (goto-char (point-min))
      (let ((iedit-auto-buffering nil)
            (iedit-auto-narrow nil)
            (iedit-auto-save-occurrence-in-kill-ring nil)
            (iedit-case-sensitive t)
            (iedit-case-sensitive-default t)
            (iedit-search-invisible t)
            (iedit-last-initial-string-global nil)
            (iedit-last-occurrence-global nil)
            (iedit-occurrence-type-global 'symbol)
            (kill-ring nil)
            (kill-ring-yank-pointer nil))
        (unwind-protect
            (funcall body)
          (when iedit-mode
            (iedit-done))
          (when iedit-rectangle-mode
            (iedit-rectangle-done)))))))
"##;

fn iedit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IEDIT_MELPA_PIN, "iedit.el")
        .expect("prepare revision-pinned Iedit source below ./tmp")
        .with_prelude(IEDIT_TEST_PRELUDE)
        .with_timeout(IEDIT_TEST_TIMEOUT)
}

fn editing_one_symbol_occurrence_propagates_a_real_refactor() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-iedit-test-run
 "(defun build-release (release-state)\n  (when release-state\n    (message \"%s\" release-state)))\n(setq release-state 'ready)\n(setq prerelease-state 'ignored)\n"
 (lambda ()
   (emacs-lisp-mode)
   (search-forward "release-state")
   (iedit-mode)
   (let ((before (neomacs-iedit-test-overlay-snapshot))
         (active (iedit-find-current-occurrence-overlay)))
     (goto-char (overlay-start active))
     (delete-region (overlay-start active) (overlay-end active))
     (insert "deploy-state")
     (run-hooks 'post-command-hook)
     (let ((after (neomacs-iedit-test-overlay-snapshot))
           (result (buffer-string)))
       (iedit-done)
       (list :before before
             :after after
             :mode iedit-mode
             :remembered iedit-last-occurrence-local
             :buffer result)))))
"##;
    let expected = expect![[
        r####"OK (:before ((:range (23 36) :column 22 :text "release-state" :category no-change) (:range (46 59) :column 8 :text "release-state" :category no-change) (:range (78 91) :column 18 :text "release-state" :category no-change) (:range (101 114) :column 6 :text "release-state" :category no-change)) :after ((:range (23 35) :column 22 :text "deploy-state" :category no-change) (:range (45 57) :column 8 :text "deploy-state" :category no-change) (:range (76 88) :column 18 :text "deploy-state" :category no-change) (:range (98 110) :column 6 :text "deploy-state" :category no-change)) :mode nil :remembered "deploy-state" :buffer "(defun build-release (deploy-state)\n  (when deploy-state\n    (message \"%s\" deploy-state)))\n(setq deploy-state 'ready)\n(setq prerelease-state 'ignored)\n")"####
    ]];
    ParityBatchCase::value(
        "editing_one_symbol_occurrence_propagates_a_real_refactor",
        elisp_form,
        expected,
    )
}

fn case_insensitive_release_rename_preserves_each_written_style() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-iedit-test-run
 "release channel\nRelease checklist\nRELEASE APPROVED\nprerelease notes\nrelease artifact\n"
 (lambda ()
   (let ((iedit-case-sensitive nil)
         (case-replace t))
     (search-forward "release")
     (iedit-mode)
     (let ((categories
            (mapcar (lambda (entry) (plist-get entry :category))
                    (neomacs-iedit-test-overlay-snapshot))))
       (iedit-replace-occurrences "deploy")
       (list :categories categories
             :count (length iedit-occurrences-overlays)
             :buffer (buffer-string))))))
"##;
    let expected = expect![[
        r####"OK (:categories (no-change cap-initial all-caps no-change) :count 4 :buffer "deploy channel\nDeploy checklist\nDEPLOY APPROVED\nprerelease notes\ndeploy artifact\n")"####
    ]];
    ParityBatchCase::value(
        "case_insensitive_release_rename_preserves_each_written_style",
        elisp_form,
        expected,
    )
}

fn function_scoped_refactor_can_be_replayed_in_a_selected_function() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-iedit-test-run
 "(defun publish (state)\n  (list state state))\n\n(defun preview (state)\n  (list state state))\n"
 (lambda ()
   (emacs-lisp-mode)
   (search-forward "state")
   (iedit-mode 0)
   (let ((local-count (length iedit-occurrences-overlays)))
     (iedit-replace-occurrences "release-state")
     (iedit-done)
     (let ((after-local (buffer-string))
           (remembered (list iedit-last-initial-string-global
                             iedit-last-occurrence-global)))
       (goto-char (point-min))
       (search-forward "(defun preview")
       (beginning-of-line)
       (mark-defun)
       (let ((unread-command-events (list ?!)))
         (iedit-execute-last-modification))
       (list :local-count local-count
             :remembered remembered
             :after-local after-local
             :after-replay (buffer-string))))))
"##;
    let expected = expect![[
        r####"OK (:local-count 3 :remembered ("state" "release-state") :after-local "(defun publish (release-state)\n  (list release-state release-state))\n\n(defun preview (state)\n  (list state state))\n" :after-replay "(defun publish (release-state)\n  (list release-state release-state))\n\n(defun preview (release-state)\n  (list release-state release-state))\n")"####
    ]];
    ParityBatchCase::value(
        "function_scoped_refactor_can_be_replayed_in_a_selected_function",
        elisp_form,
        expected,
    )
}

fn protected_and_folded_occurrences_obey_visibility_and_read_only_rules() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-iedit-test-run
 "release: protected\n* release: live\n** release: folded\n"
 (lambda ()
   (put-text-property 1 8 'read-only t)
   (outline-mode)
   (goto-char (point-min))
   (search-forward "release" nil nil 2)
   (beginning-of-line)
   (outline-hide-subtree)
   (search-forward "release")
   (let ((iedit-search-invisible nil))
       (iedit-mode)
       (let ((visible-state
              (list :editable (length iedit-occurrences-overlays)
                    :read-only (length iedit-read-only-occurrences-overlays)
                    :skipped iedit-lib-skip-invisible-count)))
         (iedit-toggle-search-invisible)
         (let ((expanded-state
                (list :search-invisible iedit-search-invisible
                      :editable (length iedit-occurrences-overlays)
                      :read-only (length iedit-read-only-occurrences-overlays)
                      :skipped iedit-lib-skip-invisible-count)))
           (let ((active (iedit-find-current-occurrence-overlay)))
             (goto-char (overlay-start active))
             (delete-region (overlay-start active) (overlay-end active))
             (insert "deploy")
             (run-hooks 'post-command-hook))
           (list :visible-state visible-state
                 :expanded-state expanded-state
                 :buffer (buffer-substring-no-properties
                          (point-min) (point-max))
                 :protected (buffer-substring-no-properties 1 8)))))))
"##;
    let expected = expect![[
        r####"OK (:visible-state (:editable 1 :read-only 1 :skipped 1) :expanded-state (:search-invisible open :editable 2 :read-only 1 :skipped 0) :buffer "release: protected\n* deploy: live\n** deploy: folded\n" :protected "release")"####
    ]];
    ParityBatchCase::value(
        "protected_and_folded_occurrences_obey_visibility_and_read_only_rules",
        elisp_form,
        expected,
    )
}

fn filtered_log_view_edits_only_the_selected_occurrences() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-iedit-test-run
 "draft alpha\ncontext one\ndraft beta\ncontext two\ncontext three\ndraft gamma\n"
 (lambda ()
   (search-forward "draft")
   (iedit-mode)
   (iedit-show/hide-context-lines 0)
   (let ((hidden (neomacs-iedit-test-hidden-ranges)))
     (iedit-next-occurrence 1)
     (let ((excluded-point (point)))
       (iedit-toggle-selection)
       (iedit-goto-first-occurrence)
       (iedit-replace-occurrences "ready")
       (let ((result (buffer-string))
             (remaining (neomacs-iedit-test-overlay-snapshot)))
         (iedit-show/hide-context-lines)
         (list :hidden hidden
               :excluded-point excluded-point
               :remaining remaining
               :hiding iedit-hiding
               :buffer result))))))
"##;
    let expected = expect![[
        r####"OK (:hidden ((13 24 "context one") (36 61 "context two\ncontext three")) :excluded-point 30 :remaining ((:range (1 6) :column 0 :text "ready" :category no-change) (:range (62 67) :column 0 :text "ready" :category no-change)) :hiding nil :buffer "ready alpha\ncontext one\ndraft beta\ncontext two\ncontext three\nready gamma\n")"####
    ]];
    ParityBatchCase::value(
        "filtered_log_view_edits_only_the_selected_occurrences",
        elisp_form,
        expected,
    )
}

fn buffered_manifest_edit_applies_atomically_and_undoes_as_one_change() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-iedit-test-run
 "artifact = release\nchecksum = release\nchannel = release\n"
 (lambda ()
   (setq buffer-undo-list nil)
   (push nil buffer-undo-list)
   (search-forward "release")
   (iedit-mode)
   (iedit-toggle-buffering)
   (insert "-v2")
   (run-hooks 'post-command-hook)
   (let ((during (buffer-string)))
     (iedit-toggle-buffering)
     (let ((applied (buffer-string))
           (applied-point (point)))
       (undo-boundary)
       (undo 1)
       (list :during during
             :applied applied
             :applied-point applied-point
             :after-undo (buffer-string)
             :undo-point (point)
             :same-length (iedit-same-length))))))
"##;
    let expected = expect![[
        r####"OK (:during "artifact = release-v2\nchecksum = release\nchannel = release\n" :applied "artifact = release-v2\nchecksum = release-v2\nchannel = release-v2\n" :applied-point 22 :after-undo "artifact = release\nchecksum = release\nchannel = release\n" :undo-point 19 :same-length t)"####
    ]];
    ParityBatchCase::value(
        "buffered_manifest_edit_applies_atomically_and_undoes_as_one_change",
        elisp_form,
        expected,
    )
}

fn rectangular_status_edit_updates_every_report_row() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-iedit-test-run
 "dev   pending alice\nprod  pending bob\nqa    pending carol\n"
 (lambda ()
   (search-forward "pending")
   (let ((beginning (match-beginning 0)))
     (goto-char (point-max))
     (search-backward "pending")
     (let ((end (match-end 0)))
       (iedit-rectangle-mode beginning end)
       (let ((before (neomacs-iedit-test-overlay-snapshot))
             (active (car (sort (copy-sequence iedit-occurrences-overlays)
                                (lambda (left right)
                                  (< (overlay-start left)
                                     (overlay-start right)))))))
         (goto-char (overlay-start active))
         (delete-region (overlay-start active) (overlay-end active))
         (insert "ready")
         (run-hooks 'post-command-hook)
         (list :before before
               :after (neomacs-iedit-test-overlay-snapshot)
               :rectangle
               (mapcar #'marker-position iedit-rectangle)
               :buffer (buffer-string)))))))
"##;
    let expected = expect![[
        r####"OK (:before ((:range (7 14) :column 6 :text "pending" :category no-change) (:range (27 34) :column 6 :text "pending" :category no-change) (:range (45 52) :column 6 :text "pending" :category no-change)) :after ((:range (7 12) :column 6 :text "ready" :category no-change) (:range (25 30) :column 6 :text "ready" :category no-change) (:range (41 46) :column 6 :text "ready" :category no-change)) :rectangle (7 46) :buffer "dev   ready alice\nprod  ready bob\nqa    ready carol\n")"####
    ]];
    ParityBatchCase::value(
        "rectangular_status_edit_updates_every_report_row",
        elisp_form,
        expected,
    )
}

#[test]
fn iedit_package_batch() {
    assert_oracle_batch_cases(
        iedit_oracle(),
        "iedit-package-batch",
        "Iedit",
        &[
            editing_one_symbol_occurrence_propagates_a_real_refactor(),
            case_insensitive_release_rename_preserves_each_written_style(),
            function_scoped_refactor_can_be_replayed_in_a_selected_function(),
            protected_and_folded_occurrences_obey_visibility_and_read_only_rules(),
            filtered_log_view_edits_only_the_selected_occurrences(),
            buffered_manifest_edit_applies_atomically_and_undoes_as_one_change(),
            rectangular_status_edit_updates_every_report_row(),
        ],
    );
}
