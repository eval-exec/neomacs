use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_VISUAL_MARK_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-visual-mark-mode)

(defconst neomacs-evil-visual-mark-test-initial-global-markers
  (copy-tree (default-value 'evil-markers-alist)))

(defun neomacs-evil-visual-mark-test-reset ()
  "Restore package and Evil marker state between parity workflows."
  (when evil-visual-mark-mode
    (evil-visual-mark-mode -1))
  (evil-visual-mark-cleanup)
  (setq-default evil-markers-alist
                (copy-tree neomacs-evil-visual-mark-test-initial-global-markers)))

(defun neomacs-evil-visual-mark-test-call-with-buffers (specs function)
  "Create Evil buffers from SPECS, then call FUNCTION with those buffers."
  (neomacs-evil-visual-mark-test-reset)
  (let ((buffers
         (mapcar
          (lambda (spec)
            (when-let ((existing (get-buffer (car spec))))
              (kill-buffer existing))
            (let ((buffer (generate-new-buffer (car spec))))
              (with-current-buffer buffer
                (insert (cdr spec))
                (goto-char (point-min))
                (evil-local-mode 1)
                (evil-normal-state))
              buffer))
          specs)))
    (unwind-protect
        (funcall function buffers)
      (when evil-visual-mark-mode
        (evil-visual-mark-mode -1))
      (dolist (buffer buffers)
        (when (buffer-live-p buffer)
          (with-current-buffer buffer
            (evil-local-mode -1))
          (kill-buffer buffer)))
      (neomacs-evil-visual-mark-test-reset))))

(defun neomacs-evil-visual-mark-test-item-state (item)
  "Return a deterministic semantic description of visual-mark ITEM."
  (let* ((key (car item))
         (char (car key))
         (owner (cdr key))
         (overlay (cdr item))
         (overlay-buffer (and (overlayp overlay) (overlay-buffer overlay)))
         (before-string (and (overlayp overlay)
                             (overlay-get overlay 'before-string))))
    (list :mark (char-to-string char)
          :scope (if (eq owner 'global) 'global 'local)
          :owner (if (bufferp owner) (buffer-name owner) owner)
          :live (and overlay-buffer t)
          :buffer (and overlay-buffer (buffer-name overlay-buffer))
          :start (and (overlayp overlay) (overlay-start overlay))
          :end (and (overlayp overlay) (overlay-end overlay))
          :label (and before-string (substring-no-properties before-string))
          :face (and (stringp before-string)
                     (> (length before-string) 0)
                     (get-text-property 0 'face before-string)))))

(defun neomacs-evil-visual-mark-test-states ()
  "Return all package overlays ordered by mark and owning buffer."
  (sort (mapcar #'neomacs-evil-visual-mark-test-item-state
                evil-visual-mark-overlay-alist)
        (lambda (left right)
          (string< (format "%s/%s" (plist-get left :mark)
                           (plist-get left :owner))
                   (format "%s/%s" (plist-get right :mark)
                           (plist-get right :owner))))))

(defun neomacs-evil-visual-mark-test-find-item (char owner)
  "Find the overlay entry for CHAR and OWNER."
  (cl-find-if (lambda (item) (equal (car item) (cons char owner)))
              evil-visual-mark-overlay-alist))
"####;

fn enabling_on_a_release_plan_renders_named_marks_and_respects_exclusions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-visual-mark-test-call-with-buffers
 '(("*evil-visual-mark-plan*" .
    "validate config\nbuild artifact\nTODO review\ndeploy release\n"))
 (lambda (buffers)
   (with-current-buffer (car buffers)
     (goto-char (point-min))
     (search-forward "validate")
     (evil-set-marker ?a (match-beginning 0))
     (search-forward "TODO")
     (evil-set-marker ?\[ (match-beginning 0))
     (search-forward "deploy")
     (evil-set-marker ?b (match-beginning 0))
     (evil-visual-mark-mode 1)
     (list :buffer (buffer-string)
           :marks (neomacs-evil-visual-mark-test-states)
           :marker-lines
           (mapcar (lambda (char)
                     (save-excursion
                       (goto-char (evil-get-marker char))
                       (list (char-to-string char)
                             (line-number-at-pos)
                             (buffer-substring-no-properties
                              (line-beginning-position)
                              (line-end-position)))))
                   '(?a ?\[ ?b))))))
"####;
    let expected = expect![[
        r#"OK (:buffer "validate config\nbuild artifact\nTODO review\ndeploy release\n" :marks ((:mark "[" :scope local :owner "*evil-visual-mark-plan*" :live t :buffer "*evil-visual-mark-plan*" :start 32 :end 32 :label nil :face nil) (:mark "a" :scope local :owner "*evil-visual-mark-plan*" :live t :buffer "*evil-visual-mark-plan*" :start 1 :end 1 :label "`a" :face evil-visual-mark-face) (:mark "b" :scope local :owner "*evil-visual-mark-plan*" :live t :buffer "*evil-visual-mark-plan*" :start 44 :end 44 :label "`b" :face evil-visual-mark-face)) :marker-lines (("a" 1 "validate config") ("[" 3 "TODO review") ("b" 4 "deploy release")))"#
    ]];
    ParityBatchCase::value(
        "enabling_on_a_release_plan_renders_named_marks_and_respects_exclusions",
        elisp_form,
        expected,
    )
}

fn moving_a_live_mark_replaces_the_overlay_after_surrounding_edits() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-visual-mark-test-call-with-buffers
 '(("*evil-visual-mark-move*" .
    "owner reviews\nartifact ready\ndeploy stable\n"))
 (lambda (buffers)
   (with-current-buffer (car buffers)
     (evil-visual-mark-mode 1)
     (goto-char (point-min))
     (search-forward "artifact")
     (evil-set-marker ?a (match-beginning 0))
     (let ((old-overlay (cdr (evil-marker-get-item ?a))))
       (goto-char (point-min))
       (insert "URGENT: ")
       (let ((after-prefix (neomacs-evil-visual-mark-test-states)))
         (goto-char (point-min))
         (search-forward "deploy")
         (evil-set-marker ?a (match-beginning 0))
         (list :text (buffer-string)
               :after-prefix after-prefix
               :after-move (neomacs-evil-visual-mark-test-states)
               :old-overlay
               (list :live (and (overlay-buffer old-overlay) t)
                     :start (overlay-start old-overlay)
                     :end (overlay-end old-overlay))))))))
"####;
    let expected = expect![[
        r#"OK (:text "URGENT: owner reviews\nartifact ready\ndeploy stable\n" :after-prefix ((:mark "a" :scope local :owner "*evil-visual-mark-move*" :live t :buffer "*evil-visual-mark-move*" :start 23 :end 23 :label "`a" :face evil-visual-mark-face)) :after-move ((:mark "a" :scope local :owner "*evil-visual-mark-move*" :live t :buffer "*evil-visual-mark-move*" :start 38 :end 38 :label "`a" :face evil-visual-mark-face)) :old-overlay (:live nil :start nil :end nil))"#
    ]];
    ParityBatchCase::value(
        "moving_a_live_mark_replaces_the_overlay_after_surrounding_edits",
        elisp_form,
        expected,
    )
}

fn local_and_global_marks_follow_their_real_owners_across_project_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-visual-mark-test-call-with-buffers
 '(("*evil-visual-mark-source*" . "source entry\nrelease call\n")
   ("*evil-visual-mark-tests*" . "test entry\nfailing scenario\n"))
 (lambda (buffers)
   (let ((source (nth 0 buffers))
         (tests (nth 1 buffers)))
     (with-current-buffer source
       (evil-visual-mark-mode 1)
       (goto-char (point-min))
       (search-forward "source")
       (evil-set-marker ?a (match-beginning 0)))
     (with-current-buffer tests
       (goto-char (point-min))
       (search-forward "failing")
       (evil-set-marker ?a (match-beginning 0))
       (evil-set-marker ?A (match-beginning 0)))
     (let ((before (neomacs-evil-visual-mark-test-states))
           (old-global
            (cdr (neomacs-evil-visual-mark-test-find-item ?A 'global))))
       (with-current-buffer source
         (goto-char (point-min))
         (search-forward "release")
         (evil-set-marker ?A (match-beginning 0)))
       (list :before before
             :after (neomacs-evil-visual-mark-test-states)
             :old-global-live (and (overlay-buffer old-global) t)
             :source-local
             (with-current-buffer source
               (let ((marker (evil-get-marker ?a t)))
                 (list (buffer-name (marker-buffer marker))
                       (marker-position marker))))
             :tests-local
             (with-current-buffer tests
               (let ((marker (evil-get-marker ?a t)))
                 (list (buffer-name (marker-buffer marker))
                       (marker-position marker))))
             :global
             (with-current-buffer tests
               (let ((marker (evil-get-marker ?A t)))
                 (list (buffer-name (marker-buffer marker))
                       (marker-position marker)))))))))
"####;
    let expected = expect![[
        r#"OK (:before ((:mark "A" :scope global :owner global :live t :buffer "*evil-visual-mark-tests*" :start 12 :end 12 :label "`A" :face evil-visual-mark-face) (:mark "a" :scope local :owner "*evil-visual-mark-source*" :live t :buffer "*evil-visual-mark-source*" :start 1 :end 1 :label "`a" :face evil-visual-mark-face) (:mark "a" :scope local :owner "*evil-visual-mark-tests*" :live t :buffer "*evil-visual-mark-tests*" :start 12 :end 12 :label "`a" :face evil-visual-mark-face)) :after ((:mark "A" :scope global :owner global :live t :buffer "*evil-visual-mark-source*" :start 14 :end 14 :label "`A" :face evil-visual-mark-face) (:mark "a" :scope local :owner "*evil-visual-mark-source*" :live t :buffer "*evil-visual-mark-source*" :start 1 :end 1 :label "`a" :face evil-visual-mark-face) (:mark "a" :scope local :owner "*evil-visual-mark-tests*" :live t :buffer "*evil-visual-mark-tests*" :start 12 :end 12 :label "`a" :face evil-visual-mark-face)) :old-global-live nil :source-local ("*evil-visual-mark-source*" 1) :tests-local ("*evil-visual-mark-tests*" 12) :global ("*evil-visual-mark-source*" 14))"#
    ]];
    ParityBatchCase::value(
        "local_and_global_marks_follow_their_real_owners_across_project_buffers",
        elisp_form,
        expected,
    )
}

fn evil_state_changes_hide_and_restore_only_configured_visible_labels() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-visual-mark-test-call-with-buffers
 '(("*evil-visual-mark-state*" . "draft\nreview\napproved\n"))
 (lambda (buffers)
   (with-current-buffer (car buffers)
     (goto-char (point-min))
     (evil-set-marker ?a (point))
     (search-forward "review")
     (evil-set-marker ?b (match-beginning 0))
     (search-forward "approved")
     (evil-set-marker ?c (match-beginning 0))
     (let ((evil-visual-mark-exclude-marks '("b")))
       (evil-visual-mark-mode 1)
       (let ((normal (neomacs-evil-visual-mark-test-states)))
         (evil-insert-state)
         (let ((insert (neomacs-evil-visual-mark-test-states)))
           (evil-normal-state)
           (list :normal normal
                 :insert insert
                 :restored (neomacs-evil-visual-mark-test-states)
                 :state evil-state)))))))
"####;
    let expected = expect![[
        r#"OK (:normal ((:mark "a" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 1 :end 1 :label "`a" :face evil-visual-mark-face) (:mark "b" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 7 :end 7 :label nil :face nil) (:mark "c" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 14 :end 14 :label "`c" :face evil-visual-mark-face)) :insert ((:mark "a" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 1 :end 1 :label "" :face nil) (:mark "b" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 7 :end 7 :label "" :face nil) (:mark "c" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 14 :end 14 :label "" :face nil)) :restored ((:mark "^" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 22 :end 22 :label "`^" :face evil-visual-mark-face) (:mark "a" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 1 :end 1 :label "`a" :face evil-visual-mark-face) (:mark "b" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 7 :end 7 :label "" :face nil) (:mark "c" :scope local :owner "*evil-visual-mark-state*" :live t :buffer "*evil-visual-mark-state*" :start 14 :end 14 :label "`c" :face evil-visual-mark-face)) :state normal)"#
    ]];
    ParityBatchCase::value(
        "evil_state_changes_hide_and_restore_only_configured_visible_labels",
        elisp_form,
        expected,
    )
}

fn deleting_selected_evil_marks_rebuilds_only_the_remaining_visual_marks() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-visual-mark-test-call-with-buffers
 '(("*evil-visual-mark-delete*" . "triage\nimplement\nverify\nship\n"))
 (lambda (buffers)
   (with-current-buffer (car buffers)
     (goto-char (point-min))
     (evil-set-marker ?a (point))
     (search-forward "implement")
     (evil-set-marker ?b (match-beginning 0))
     (search-forward "verify")
     (evil-set-marker ?A (match-beginning 0))
     (evil-visual-mark-mode 1)
     (let ((old-overlays (mapcar #'cdr evil-visual-mark-overlay-alist)))
       (evil-delete-marks "aA")
       (list :remaining (neomacs-evil-visual-mark-test-states)
             :old-overlays-live
             (mapcar (lambda (overlay) (and (overlay-buffer overlay) t))
                     old-overlays)
             :markers
             (mapcar (lambda (char)
                       (let ((marker (evil-get-marker char t)))
                         (list (char-to-string char)
                               (and (markerp marker)
                                    (marker-position marker)))))
                     '(?a ?b ?A)))))))
"####;
    let expected = expect![[
        r#"OK (:remaining ((:mark "b" :scope local :owner "*evil-visual-mark-delete*" :live t :buffer "*evil-visual-mark-delete*" :start 8 :end 8 :label "`b" :face evil-visual-mark-face)) :old-overlays-live (nil nil) :markers (("a" nil) ("b" 8) ("A" nil)))"#
    ]];
    ParityBatchCase::value(
        "deleting_selected_evil_marks_rebuilds_only_the_remaining_visual_marks",
        elisp_form,
        expected,
    )
}

fn disabling_stops_live_updates_and_reenabling_rehydrates_existing_marks() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-visual-mark-test-call-with-buffers
 '(("*evil-visual-mark-lifecycle*" . "open\nwork\nclose\n"))
 (lambda (buffers)
   (with-current-buffer (car buffers)
     (goto-char (point-min))
     (evil-set-marker ?a (point))
     (evil-visual-mark-mode 1)
     (let ((first-overlay (cdr (evil-marker-get-item ?a))))
       (evil-visual-mark-mode -1)
       (search-forward "close")
       (evil-set-marker ?b (match-beginning 0))
       (let ((disabled
              (list :mode evil-visual-mark-mode
                    :overlays evil-visual-mark-overlay-alist
                    :first-live (and (overlay-buffer first-overlay) t)
                    :set-advice
                    (and (advice-member-p
                          #'evil-set-marker--visual-mark-update
                          'evil-set-marker)
                         t)
                    :delete-advice
                    (and (advice-member-p
                          #'evil-delete-marks--visual-mark-update
                          'evil-delete-marks)
                         t)
                    :exit-hook
                    (and (memq 'evil-visual-mark-hide
                               evil-normal-state-exit-hook)
                         t))))
         (evil-visual-mark-mode 1)
         (list :disabled disabled
               :reenabled
               (list :mode evil-visual-mark-mode
                     :marks (neomacs-evil-visual-mark-test-states)
                     :set-advice
                     (and (advice-member-p
                           #'evil-set-marker--visual-mark-update
                           'evil-set-marker)
                          t)
                     :delete-advice
                     (and (advice-member-p
                           #'evil-delete-marks--visual-mark-update
                           'evil-delete-marks)
                          t)
                     :exit-hook
                     (and (memq 'evil-visual-mark-hide
                                evil-normal-state-exit-hook)
                          t))))))))
"####;
    let expected = expect![[
        r#"OK (:disabled (:mode nil :overlays nil :first-live nil :set-advice nil :delete-advice nil :exit-hook nil) :reenabled (:mode t :marks ((:mark "a" :scope local :owner "*evil-visual-mark-lifecycle*" :live t :buffer "*evil-visual-mark-lifecycle*" :start 1 :end 1 :label "`a" :face evil-visual-mark-face) (:mark "b" :scope local :owner "*evil-visual-mark-lifecycle*" :live t :buffer "*evil-visual-mark-lifecycle*" :start 11 :end 11 :label "`b" :face evil-visual-mark-face)) :set-advice t :delete-advice t :exit-hook t))"#
    ]];
    ParityBatchCase::value(
        "disabling_stops_live_updates_and_reenabling_rehydrates_existing_marks",
        elisp_form,
        expected,
    )
}

fn editing_at_a_mark_exposes_marker_and_zero_length_overlay_boundary_semantics() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-evil-visual-mark-test-call-with-buffers
 '(("*evil-visual-mark-edit*" . "queued deployment\n"))
 (lambda (buffers)
   (with-current-buffer (car buffers)
     (goto-char (point-min))
     (search-forward "deployment")
     (evil-set-marker ?q (match-beginning 0) t)
     (evil-visual-mark-mode 1)
     (let* ((marker (evil-get-marker ?q t))
            (overlay (cdr (evil-marker-get-item ?q)))
            (before (neomacs-evil-visual-mark-test-states)))
       (goto-char (overlay-start overlay))
       (insert "urgent ")
       (list :text (buffer-string)
             :before before
             :after (neomacs-evil-visual-mark-test-states)
             :marker
             (list :position (marker-position marker)
                   :advance (marker-insertion-type marker))
             :overlay-length (- (overlay-end overlay)
                                (overlay-start overlay)))))))
"####;
    let expected = expect![[
        r#"OK (:text "queued urgent deployment\n" :before ((:mark "q" :scope local :owner "*evil-visual-mark-edit*" :live t :buffer "*evil-visual-mark-edit*" :start 8 :end 8 :label "`q" :face evil-visual-mark-face)) :after ((:mark "q" :scope local :owner "*evil-visual-mark-edit*" :live t :buffer "*evil-visual-mark-edit*" :start 8 :end 8 :label "`q" :face evil-visual-mark-face)) :marker (:position 15 :advance t) :overlay-length 0)"#
    ]];
    ParityBatchCase::value(
        "editing_at_a_mark_exposes_marker_and_zero_length_overlay_boundary_semantics",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_visual_mark_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(EVIL_VISUAL_MARK_MODE_MELPA_PIN, "evil-visual-mark-mode.el")
            .expect("prepare revision-pinned Evil Visual Mark Mode source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "evil-visual-mark-mode-package-batch",
        "Evil Visual Mark Mode",
        &[
            enabling_on_a_release_plan_renders_named_marks_and_respects_exclusions(),
            moving_a_live_mark_replaces_the_overlay_after_surrounding_edits(),
            local_and_global_marks_follow_their_real_owners_across_project_buffers(),
            evil_state_changes_hide_and_restore_only_configured_visible_labels(),
            deleting_selected_evil_marks_rebuilds_only_the_remaining_visual_marks(),
            disabling_stops_live_updates_and_reenabling_rehydrates_existing_marks(),
            editing_at_a_mark_exposes_marker_and_zero_length_overlay_boundary_semantics(),
        ],
    );
}
