use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, VOLATILE_HIGHLIGHTS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const VOLATILE_HIGHLIGHTS_TEST_TIMEOUT: Duration = Duration::from_secs(90);
const VOLATILE_HIGHLIGHTS_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'xref)
(require 'volatile-highlights)

(setq vhl/animation-style 'static
      vhl/highlight-zero-width-ranges t
      xref-after-jump-hook nil
      xref-after-return-hook nil
      xref-history-storage #'xref-global-history)

(defun neomacs-vhl-test-overlays (&optional buffer)
  "Describe Volatile Highlights overlays in BUFFER in source order."
  (with-current-buffer (or buffer (current-buffer))
    (mapcar
     (lambda (overlay)
       (list :range (list (overlay-start overlay) (overlay-end overlay))
             :text (buffer-substring-no-properties
                    (overlay-start overlay) (overlay-end overlay))
             :face (overlay-get overlay 'face)
             :priority (overlay-get overlay 'priority)
             :volatile (overlay-get overlay 'volatile-highlights)))
     (sort
      (cl-remove-if-not
       (lambda (overlay) (overlay-get overlay 'volatile-highlights))
       (overlays-in (point-min) (point-max)))
      (lambda (left right)
        (or (< (overlay-start left) (overlay-start right))
            (and (= (overlay-start left) (overlay-start right))
                 (< (overlay-end left) (overlay-end right)))))))))

(defun neomacs-vhl-test-clear-on-next-command ()
  "Run the real pre-command hook and report VHL's lifecycle."
  (let ((installed (and (memq #'vhl/clear-all pre-command-hook) t)))
    (run-hooks 'pre-command-hook)
    (list :hook-was-installed installed
          :remaining (neomacs-vhl-test-overlays)
          :hook-remains (and (memq #'vhl/clear-all pre-command-hook) t))))

(defun neomacs-vhl-test-reset ()
  "Restore the global VHL and xref state used by this parity batch."
  (when volatile-highlights-mode
    (volatile-highlights-mode -1))
  (vhl/clear-all)
  (setq vhl/.after-change-hook-depth 0
        vhl/use-xref-extension-p nil)
  (remove-hook 'after-change-functions #'vhl/.make-vhl-on-change)
  (xref-global-history (cons nil nil)))

(defun neomacs-vhl-test-duplicate-release-line ()
  "Duplicate the current release line, like a small third-party command."
  (interactive)
  (let ((line (buffer-substring-no-properties
               (line-beginning-position) (line-beginning-position 2))))
    (goto-char (line-beginning-position 2))
    (insert line)))

(vhl/define-extension 'neomacs-test 'neomacs-vhl-test-duplicate-release-line)
(vhl/install-extension 'neomacs-test)

(defvar neomacs-vhl-test-xref-target nil)

(defun neomacs-vhl-test-xref-backend ()
  "Return the deterministic backend used by the xref workflow."
  'neomacs-vhl-test)

(cl-defmethod xref-backend-identifier-at-point ((_backend (eql neomacs-vhl-test)))
  (thing-at-point 'symbol t))

(cl-defmethod xref-backend-definitions ((_backend (eql neomacs-vhl-test)) identifier)
  (list (xref-make identifier
                   (xref-make-file-location neomacs-vhl-test-xref-target 1 7))))

(defun neomacs-vhl-test-root (name)
  "Create and return a deterministic sandbox directory for NAME."
  (let ((root (expand-file-name
               (concat "volatile-highlights-" name "/")
               (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-vhl-test-write (path contents)
  "Write CONTENTS to PATH and return PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-vhl-test-visit (path needle)
  "Visit PATH and move point to the beginning of NEEDLE."
  (switch-to-buffer (find-file-noselect path))
  (goto-char (point-min))
  (search-forward needle)
  (goto-char (match-beginning 0))
  (current-buffer))

(defun neomacs-vhl-test-location (root)
  "Describe the current file location relative to ROOT."
  (list :file (file-relative-name buffer-file-name root)
        :line (line-number-at-pos)
        :column (current-column)
        :symbol (thing-at-point 'symbol t)))

(defun neomacs-vhl-test-marker (marker root)
  "Describe MARKER relative to ROOT."
  (let ((buffer (marker-buffer marker)))
    (when buffer
      (with-current-buffer buffer
        (save-excursion
          (goto-char marker)
          (list :file (file-relative-name buffer-file-name root)
                :position (marker-position marker)
                :line (line-number-at-pos)
                :column (current-column)))))))

(defun neomacs-vhl-test-history (root)
  "Describe the exact global xref history relative to ROOT."
  (let ((history (xref-global-history)))
    (list :backward
          (mapcar (lambda (marker) (neomacs-vhl-test-marker marker root))
                  (car history))
          :forward
          (mapcar (lambda (marker) (neomacs-vhl-test-marker marker root))
                  (cdr history)))))

(defun neomacs-vhl-test-cleanup-files (root)
  "Kill buffers below ROOT and remove their deterministic files."
  (dolist (buffer (buffer-list))
    (when (and (buffer-file-name buffer)
               (string-prefix-p root (buffer-file-name buffer)))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (neomacs-vhl-test-reset)
  (when (file-exists-p root)
    (delete-directory root t)))
"###;

fn volatile_highlights_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(VOLATILE_HIGHLIGHTS_MELPA_PIN, "volatile-highlights.el")
        .expect("prepare revision-pinned Volatile Highlights source below ./tmp")
        .with_prelude(VOLATILE_HIGHLIGHTS_TEST_PRELUDE)
        .with_timeout(VOLATILE_HIGHLIGHTS_TEST_TIMEOUT)
}

fn clipboard_yank_and_yank_pop_highlight_the_inserted_release_value_then_clear_on_command()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (neomacs-vhl-test-reset)
  (unwind-protect
      (progn
        (insert "channel=")
        (setq kill-ring '("stable" "canary"))
        (volatile-highlights-mode 1)
        (goto-char (point-max))
        (let ((inhibit-message t))
          (yank))
        (let ((after-yank
               (list :buffer (buffer-string)
                     :point (point)
                     :overlays (neomacs-vhl-test-overlays)))
              (clear-after-yank (neomacs-vhl-test-clear-on-next-command)))
          (setq last-command 'yank)
          (let ((inhibit-message t))
            (yank-pop 1))
          (list :after-yank after-yank
                :clear-after-yank clear-after-yank
                :after-yank-pop
                (list :buffer (buffer-string)
                      :point (point)
                      :overlays (neomacs-vhl-test-overlays))
                :clear-after-yank-pop
                (neomacs-vhl-test-clear-on-next-command))))
    (neomacs-vhl-test-reset)))
"###;
    let expected = expect![[
        r#"OK (:after-yank (:buffer "channel=stable" :point 15 :overlays ((:range (9 15) :text "stable" :face vhl/default-face :priority 1 :volatile t))) :clear-after-yank (:hook-was-installed t :remaining nil :hook-remains nil) :after-yank-pop (:buffer "channel=canary" :point 15 :overlays ((:range (8 9) :text "=" :face vhl/default-face :priority 1 :volatile t) (:range (9 15) :text "canary" :face vhl/default-face :priority 1 :volatile t))) :clear-after-yank-pop (:hook-was-installed t :remaining nil :hook-remains nil))"#
    ]];
    ParityBatchCase::value(
        "clipboard_yank_and_yank_pop_highlight_the_inserted_release_value_then_clear_on_command",
        elisp_form,
        expected,
    )
}

fn release_replacement_and_word_transposition_report_every_changed_range() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (neomacs-vhl-test-reset)
  (unwind-protect
      (progn
        (insert "REL-1 pending; REL-2 pending\nblue green\n")
        (volatile-highlights-mode 1)
        (goto-char (point-min))
        (let ((inhibit-message t))
          (replace-string "pending" "ready"))
        (let ((replacement
               (list :buffer (buffer-string)
                     :point (point)
                     :overlays (neomacs-vhl-test-overlays)))
              (clear-after-replacement (neomacs-vhl-test-clear-on-next-command)))
          (goto-char (point-min))
          (forward-line 1)
          (forward-word 1)
          (let ((inhibit-message t))
            (transpose-words 1))
          (list :replacement replacement
                :clear-after-replacement clear-after-replacement
                :transposition
                (list :buffer (buffer-string)
                      :point (point)
                      :overlays (neomacs-vhl-test-overlays))
                :clear-after-transposition
                (neomacs-vhl-test-clear-on-next-command))))
    (neomacs-vhl-test-reset)))
"###;
    let expected = expect![[
        r#"OK (:replacement (:buffer "REL-1 ready; REL-2 ready\nblue green\n" :point 25 :overlays ((:range (7 12) :text "ready" :face vhl/default-face :priority 1 :volatile t) (:range (20 25) :text "ready" :face vhl/default-face :priority 1 :volatile t))) :clear-after-replacement (:hook-was-installed t :remaining nil :hook-remains nil) :transposition (:buffer "REL-1 ready; REL-2 ready\ngreen blue\n" :point 36 :overlays ((:range (26 27) :text "g" :face vhl/default-face :priority 1 :volatile t) (:range (26 31) :text "green" :face vhl/default-face :priority 1 :volatile t) (:range (32 36) :text "blue" :face vhl/default-face :priority 1 :volatile t) (:range (36 37) :text "\n" :face vhl/default-face :priority 1 :volatile t))) :clear-after-transposition (:hook-was-installed t :remaining nil :hook-remains nil))"#
    ]];
    ParityBatchCase::value(
        "release_replacement_and_word_transposition_report_every_changed_range",
        elisp_form,
        expected,
    )
}

fn deletion_undo_and_nested_kill_track_exact_feedback_without_leaking_change_hooks()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (neomacs-vhl-test-reset)
  (unwind-protect
      (progn
        (buffer-disable-undo)
        (insert "release: alpha beta gamma")
        (buffer-enable-undo)
        (undo-boundary)
        (volatile-highlights-mode 1)
        (goto-char (point-min))
        (search-forward "beta")
        (delete-region (match-beginning 0) (match-end 0))
        (undo-boundary)
        (let ((deletion
               (list :buffer (buffer-string)
                     :overlays (neomacs-vhl-test-overlays)
                     :tracking-depth vhl/.after-change-hook-depth
                     :change-hook-active
                     (and (memq #'vhl/.make-vhl-on-change after-change-functions) t)))
              (clear-after-delete (neomacs-vhl-test-clear-on-next-command)))
          (let ((inhibit-message t))
            (undo))
          (let ((restoration
                 (list :buffer (buffer-string)
                       :overlays (neomacs-vhl-test-overlays)
                       :tracking-depth vhl/.after-change-hook-depth
                       :change-hook-active
                       (and (memq #'vhl/.make-vhl-on-change after-change-functions) t)))
                (clear-after-undo (neomacs-vhl-test-clear-on-next-command)))
            (goto-char (point-min))
            (search-forward "alpha")
            (kill-region (match-beginning 0) (match-end 0))
            (list :deletion deletion
                  :clear-after-delete clear-after-delete
                  :restoration restoration
                  :clear-after-undo clear-after-undo
                  :kill
                  (list :buffer (buffer-string)
                        :kill-ring-head (car kill-ring)
                        :overlays (neomacs-vhl-test-overlays)
                        :tracking-depth vhl/.after-change-hook-depth
                        :change-hook-active
                        (and (memq #'vhl/.make-vhl-on-change after-change-functions) t))))))
    (neomacs-vhl-test-reset)))
"###;
    let expected = expect![[
        r#"OK (:deletion (:buffer "release: alpha  gamma" :overlays ((:range (16 17) :text " " :face vhl/default-face :priority 1 :volatile t)) :tracking-depth 0 :change-hook-active nil) :clear-after-delete (:hook-was-installed t :remaining nil :hook-remains nil) :restoration (:buffer "release: alpha beta gamma" :overlays ((:range (16 20) :text "beta" :face vhl/default-face :priority 1 :volatile t)) :tracking-depth 0 :change-hook-active nil) :clear-after-undo (:hook-was-installed t :remaining nil :hook-remains nil) :kill (:buffer "release:  beta gamma" :kill-ring-head "alpha" :overlays ((:range (10 11) :text " " :face vhl/default-face :priority 1 :volatile t)) :tracking-depth 0 :change-hook-active nil))"#
    ]];
    ParityBatchCase::value(
        "deletion_undo_and_nested_kill_track_exact_feedback_without_leaking_change_hooks",
        elisp_form,
        expected,
    )
}

fn generated_third_party_extension_tracks_a_real_command_and_unloads_cleanly() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (neomacs-vhl-test-reset)
  (unwind-protect
      (progn
        (insert "release: REL-42\nnext: pending\n")
        (goto-char (point-min))
        (volatile-highlights-mode 1)
        (neomacs-vhl-test-duplicate-release-line)
        (let ((enabled
               (list :buffer (buffer-string)
                     :overlays (neomacs-vhl-test-overlays)
                     :advice-installed
                     (and (advice-member-p
                           #'vhl/.advice-callback-fn/.make-vhl-on-neomacs-vhl-test-duplicate-release-line
                           'neomacs-vhl-test-duplicate-release-line)
                          t)
                     :tracking-depth vhl/.after-change-hook-depth)))
          (neomacs-vhl-test-clear-on-next-command)
          (volatile-highlights-mode -1)
          (goto-char (point-min))
          (forward-line 1)
          (neomacs-vhl-test-duplicate-release-line)
          (vhl/add-range (point-min) (line-end-position))
          (list :enabled enabled
                :disabled
                (list :buffer (buffer-string)
                      :overlays (neomacs-vhl-test-overlays)
                      :advice-installed
                      (and (advice-member-p
                            #'vhl/.advice-callback-fn/.make-vhl-on-neomacs-vhl-test-duplicate-release-line
                            'neomacs-vhl-test-duplicate-release-line)
                           t)
                      :tracking-depth vhl/.after-change-hook-depth))))
    (neomacs-vhl-test-reset)))
"###;
    let expected = expect![[
        r#"OK (:enabled (:buffer "release: REL-42\nrelease: REL-42\nnext: pending\n" :overlays ((:range (17 33) :text "release: REL-42\n" :face vhl/default-face :priority 1 :volatile t)) :advice-installed t :tracking-depth 0) :disabled (:buffer "release: REL-42\nrelease: REL-42\nrelease: REL-42\nnext: pending\n" :overlays nil :advice-installed nil :tracking-depth 0))"#
    ]];
    ParityBatchCase::value(
        "generated_third_party_extension_tracks_a_real_command_and_unloads_cleanly",
        elisp_form,
        expected,
    )
}

fn xref_definition_jump_and_return_highlight_the_symbol_at_each_real_destination() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((root (neomacs-vhl-test-root "xref"))
       (target (expand-file-name "src/release.el" root))
       (caller (expand-file-name "app/deploy.el" root)))
  (unwind-protect
      (progn
        (neomacs-vhl-test-reset)
        (neomacs-vhl-test-write
         target
         "(defun deploy-release (release)\n  (message \"deploying %s\" release))\n")
        (neomacs-vhl-test-write
         caller
         "(deploy-release \"REL-42\")\n")
        (setq neomacs-vhl-test-xref-target target
              vhl/use-xref-extension-p t)
        (neomacs-vhl-test-visit caller "deploy-release")
        (setq-local xref-backend-functions '(neomacs-vhl-test-xref-backend))
        (xref-global-history (cons nil nil))
        (volatile-highlights-mode 1)
        (execute-kbd-macro (kbd "M-."))
        (let ((destination
               (list :location (neomacs-vhl-test-location root)
                     :overlays (neomacs-vhl-test-overlays)
                     :history (neomacs-vhl-test-history root)))
              (clear-at-destination (neomacs-vhl-test-clear-on-next-command)))
          (execute-kbd-macro (kbd "M-,"))
          (list :destination destination
                :clear-at-destination clear-at-destination
                :return
                (list :location (neomacs-vhl-test-location root)
                      :overlays (neomacs-vhl-test-overlays)
                      :history (neomacs-vhl-test-history root)))))
    (neomacs-vhl-test-cleanup-files root)))
"###;
    let expected = expect![[
        r#"OK (:destination (:location (:file "src/release.el" :line 1 :column 7 :symbol "deploy-release") :overlays ((:range (8 22) :text "deploy-release" :face vhl/default-face :priority 1 :volatile t)) :history (:backward ((:file "app/deploy.el" :position 2 :line 1 :column 1)) :forward nil)) :clear-at-destination (:hook-was-installed nil :remaining nil :hook-remains nil) :return (:location (:file "app/deploy.el" :line 1 :column 1 :symbol "deploy-release") :overlays ((:range (2 16) :text "deploy-release" :face vhl/default-face :priority 1 :volatile t)) :history (:backward nil :forward ((:file "src/release.el" :position 8 :line 1 :column 7)))))"#
    ]];
    ParityBatchCase::value(
        "xref_definition_jump_and_return_highlight_the_symbol_at_each_real_destination",
        elisp_form,
        expected,
    )
}

#[test]
fn volatile_highlights_package_batch() {
    assert_oracle_batch_cases(
        volatile_highlights_oracle(),
        "volatile-highlights-package-batch",
        "Volatile Highlights",
        &[
            clipboard_yank_and_yank_pop_highlight_the_inserted_release_value_then_clear_on_command(
            ),
            release_replacement_and_word_transposition_report_every_changed_range(),
            deletion_undo_and_nested_kill_track_exact_feedback_without_leaking_change_hooks(),
            generated_third_party_extension_tracks_a_real_command_and_unloads_cleanly(),
            xref_definition_jump_and_return_highlight_the_symbol_at_each_real_destination(),
        ],
    );
}
