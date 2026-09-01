use std::time::Duration;

use expect_test::expect;

use crate::{COLUMN_ENFORCE_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'column-enforce-mode)

(defun neomacs-cem-test-column-at (position)
  "Return the display column at POSITION."
  (save-excursion
    (goto-char position)
    (current-column)))

(defun neomacs-cem-test-overlays ()
  "Describe Column Enforce overlays in source order."
  (mapcar
   (lambda (overlay)
     (let ((start (overlay-start overlay))
           (end (overlay-end overlay)))
       (list :line (line-number-at-pos start)
             :start start
             :end end
             :start-column (neomacs-cem-test-column-at start)
             :end-column (neomacs-cem-test-column-at end)
             :text (buffer-substring-no-properties start end)
             :face (overlay-get overlay 'face)
             :marker (overlay-get overlay 'is-cem-ov))))
   (sort (copy-sequence
          (column-enforce-get-cem-overlays-in (point-min) (point-max)))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun neomacs-cem-test-line-widths ()
  "Return each logical line's number and final display column."
  (save-excursion
    (goto-char (point-min))
    (let (widths)
      (while (< (point) (point-max))
        (let ((line (line-number-at-pos)))
          (end-of-line)
          (push (list line (current-column)) widths))
        (forward-line 1))
      (nreverse widths))))

(defun neomacs-cem-test-state ()
  "Describe the active rule, JIT registration, and warning overlays."
  (list :mode column-enforce-mode
        :column (column-enforce-get-column)
        :lighter column-enforce-mode-line-string
        :jit-registered
        (not (null (memq #'column-enforce-warn-on-region
                         jit-lock-functions)))
        :overlays (neomacs-cem-test-overlays)))
"####;

fn marks_real_code_by_display_column_across_tabs_and_wide_characters() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (setq-local tab-width 8)
  (insert
   "(defun publish-release (artifact destination)\n"
   "\t(message \"shipping λ-build to %s\" destination))\n"
   "(setq 状態 \"ready-for-production-deployment\")\n"
   "(provide 'release)\n")
  (setq-local column-enforce-column 28)
  (unwind-protect
      (progn
        (column-enforce-mode 1)
        (jit-lock-fontify-now (point-min) (point-max))
        (list :text (buffer-substring-no-properties (point-min) (point-max))
              :tab-width tab-width
              :line-widths (neomacs-cem-test-line-widths)
              :state (neomacs-cem-test-state)))
    (column-enforce-mode -1)))
"####;
    let expected = expect![[
        r#"OK (:text "(defun publish-release (artifact destination)\n\11(message \"shipping λ-build to %s\" destination))\n(setq 状態 \"ready-for-production-deployment\")\n(provide 'release)\n" :tab-width 8 :line-widths ((1 45) (2 55) (3 45) (4 18)) :state (:mode t :column 28 :lighter " 28col" :jit-registered t :overlays ((:line 1 :start 29 :end 46 :start-column 28 :end-column 45 :text "fact destination)" :face column-enforce-face :marker t) (:line 2 :start 68 :end 95 :start-column 28 :end-column 55 :text "-build to %s\" destination))" :face column-enforce-face :marker t) (:line 3 :start 122 :end 139 :start-column 28 :end-column 45 :text "tion-deployment\")" :face column-enforce-face :marker t))))"#
    ]];
    ParityBatchCase::value(
        "marks_real_code_by_display_column_across_tabs_and_wide_characters",
        elisp_form,
        expected,
    )
}

fn applies_the_comment_policy_without_exempting_strings_or_code() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert
   ";; operational commentary may exceed the release limit\n"
   "(setq release-target \"linux-x86_64-production\")\n"
   "(message \"literal ;; is not a comment and stays checked\")\n"
   "(deploy)          ;; commentary beyond the release limit\n")
  (setq-local column-enforce-column 26)
  (setq-local column-enforce-comments nil)
  (unwind-protect
      (progn
        (column-enforce-mode 1)
        (let ((comments-exempt (neomacs-cem-test-state)))
          (setq-local column-enforce-comments t)
          (column-enforce-warn-on-region (point-min) (point-max))
          (list :comments-exempt comments-exempt
                :comments-enforced (neomacs-cem-test-state)
                :text (buffer-substring-no-properties
                       (point-min) (point-max)))))
    (column-enforce-mode -1)))
"####;
    let expected = expect![[
        r#"OK (:comments-exempt (:mode t :column 26 :lighter " 26col" :jit-registered t :overlays ((:line 2 :start 82 :end 103 :start-column 26 :end-column 47 :text "x-x86_64-production\")" :face column-enforce-face :marker t) (:line 3 :start 130 :end 161 :start-column 26 :end-column 57 :text "t a comment and stays checked\")" :face column-enforce-face :marker t))) :comments-enforced (:mode t :column 26 :lighter " 26col" :jit-registered t :overlays ((:line 1 :start 27 :end 55 :start-column 26 :end-column 54 :text "may exceed the release limit" :face column-enforce-face :marker t) (:line 2 :start 82 :end 103 :start-column 26 :end-column 47 :text "x-x86_64-production\")" :face column-enforce-face :marker t) (:line 3 :start 130 :end 161 :start-column 26 :end-column 57 :text "t a comment and stays checked\")" :face column-enforce-face :marker t) (:line 4 :start 188 :end 218 :start-column 26 :end-column 56 :text "ntary beyond the release limit" :face column-enforce-face :marker t))) :text ";; operational commentary may exceed the release limit\n(setq release-target \"linux-x86_64-production\")\n(message \"literal ;; is not a comment and stays checked\")\n(deploy)          ;; commentary beyond the release limit\n")"#
    ]];
    ParityBatchCase::value(
        "applies_the_comment_policy_without_exempting_strings_or_code",
        elisp_form,
        expected,
    )
}

fn updates_warnings_after_a_developer_shortens_and_extends_a_line() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (buffer-enable-undo)
  (insert "(setq artifact-name \"nightly-linux-x86_64-release\")\n")
  (setq-local column-enforce-column 20)
  (setq-local column-enforce-face 'error)
  (let ((review-overlay (make-overlay 1 6)))
    (overlay-put review-overlay 'owner 'release-review)
    (unwind-protect
        (progn
          (column-enforce-mode 1)
          (let ((initial (neomacs-cem-test-state)))
            (erase-buffer)
            (insert "(setq artifact \"v1\")\n")
            (jit-lock-refontify (point-min) (point-max))
            (jit-lock-fontify-now (point-min) (point-max))
            (let ((shortened (neomacs-cem-test-state)))
              (goto-char (point-max))
              (forward-line -1)
              (end-of-line)
              (insert " ; verify checksum and publish atomically")
              (jit-lock-refontify (line-beginning-position) (point-max))
              (jit-lock-fontify-now (line-beginning-position) (point-max))
              (let ((extended (neomacs-cem-test-state)))
                (column-enforce-mode -1)
                (list
                 :initial initial
                 :shortened shortened
                 :extended extended
                 :disabled (neomacs-cem-test-state)
                 :review-overlay
                 (list :start (overlay-start review-overlay)
                       :end (overlay-end review-overlay)
                       :owner (overlay-get review-overlay 'owner)
                       :text
                       (buffer-substring-no-properties
                        (overlay-start review-overlay)
                        (overlay-end review-overlay)))
                 :text (buffer-substring-no-properties
                        (point-min) (point-max)))))))
      (when column-enforce-mode (column-enforce-mode -1)))))
"####;
    let expected = expect![[
        r#"OK (:initial (:mode t :column 20 :lighter " 20col" :jit-registered t :overlays ((:line 1 :start 21 :end 52 :start-column 20 :end-column 51 :text "\"nightly-linux-x86_64-release\")" :face error :marker t))) :shortened (:mode t :column 20 :lighter " 20col" :jit-registered t :overlays ((:line 2 :start 22 :end 22 :start-column 0 :end-column 0 :text "" :face error :marker t))) :extended (:mode t :column 20 :lighter " 20col" :jit-registered t :overlays ((:line 1 :start 21 :end 62 :start-column 20 :end-column 61 :text " ; verify checksum and publish atomically" :face error :marker t) (:line 2 :start 63 :end 63 :start-column 0 :end-column 0 :text "" :face error :marker t))) :disabled (:mode nil :column 20 :lighter " 20col" :jit-registered nil :overlays nil) :review-overlay (:start 1 :end 1 :owner release-review :text "") :text "(setq artifact \"v1\") ; verify checksum and publish atomically\n")"#
    ]];
    ParityBatchCase::value(
        "updates_warnings_after_a_developer_shortens_and_extends_a_line",
        elisp_form,
        expected,
    )
}

fn warning_boundaries_follow_insertions_like_live_editor_annotations() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJ-over-limit\n")
  (setq-local column-enforce-column 20)
  (unwind-protect
      (progn
        (column-enforce-mode 1)
        (let* ((overlay
                (car (column-enforce-get-cem-overlays-in
                      (point-min) (point-max))))
               (before
                (list :start (overlay-start overlay)
                      :end (overlay-end overlay)
                      :text (buffer-substring-no-properties
                             (overlay-start overlay) (overlay-end overlay)))))
          (goto-char (overlay-start overlay))
          (insert "X")
          (let ((after-front
                 (list :start (overlay-start overlay)
                       :end (overlay-end overlay)
                       :text (buffer-substring-no-properties
                              (overlay-start overlay) (overlay-end overlay)))))
            (goto-char (overlay-end overlay))
            (insert "Y")
            (list :before before
                  :after-front after-front
                  :after-rear
                  (list :start (overlay-start overlay)
                        :end (overlay-end overlay)
                        :text (buffer-substring-no-properties
                               (overlay-start overlay) (overlay-end overlay)))
                  :buffer
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))
    (column-enforce-mode -1)))
"####;
    let expected = expect![[
        r#"OK (:before (:start 21 :end 32 :text "-over-limit") :after-front (:start 22 :end 33 :text "-over-limit") :after-rear (:start 22 :end 34 :text "-over-limitY") :buffer "0123456789ABCDEFGHIJX-over-limitY\n")"#
    ]];
    ParityBatchCase::value(
        "warning_boundaries_follow_insertions_like_live_editor_annotations",
        elisp_form,
        expected,
    )
}

fn uses_contextual_limits_for_headers_continuations_and_generated_lines() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "HEADER release-candidate-ready\n"
   "  continuation metadata for deployment\n"
   "generated artifact manifest with a deliberately long immutable digest value\n"
   "body publishes checksums and signatures\n")
  (setq-local column-enforce-column 24)
  (setq-local column-enforce-column-getter
              (lambda ()
                (cond
                 ((looking-at "HEADER") 16)
                 ((looking-at "  ") 28)
                 ((looking-at "generated")
                  (error "generated metadata has no local policy"))
                 (t column-enforce-column))))
  (unwind-protect
      (progn
        (column-enforce-mode 1)
        (let ((limits
               (save-excursion
                 (goto-char (point-min))
                 (let (values)
                   (while (< (point) (point-max))
                     (push (list (line-number-at-pos)
                                 (column-enforce-get-column))
                           values)
                     (forward-line 1))
                   (nreverse values)))))
          (list :limits limits
                :widths (neomacs-cem-test-line-widths)
                :state (neomacs-cem-test-state)
                :text (buffer-substring-no-properties
                       (point-min) (point-max)))))
    (column-enforce-mode -1)))
"####;
    let expected = expect![[
        r#"OK (:limits ((1 16) (2 28) (3 80) (4 24)) :widths ((1 30) (2 38) (3 75) (4 39)) :state (:mode t :column 24 :lighter " 24col" :jit-registered t :overlays ((:line 1 :start 17 :end 31 :start-column 16 :end-column 30 :text "andidate-ready" :face column-enforce-face :marker t) (:line 2 :start 60 :end 70 :start-column 28 :end-column 38 :text "deployment" :face column-enforce-face :marker t) (:line 4 :start 171 :end 186 :start-column 24 :end-column 39 :text " and signatures" :face column-enforce-face :marker t))) :text "HEADER release-candidate-ready\n  continuation metadata for deployment\ngenerated artifact manifest with a deliberately long immutable digest value\nbody publishes checksums and signatures\n")"#
    ]];
    ParityBatchCase::value(
        "uses_contextual_limits_for_headers_continuations_and_generated_lines",
        elisp_form,
        expected,
    )
}

fn interactive_and_predefined_rules_drive_the_review_lifecycle() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert
   "(message \"publish release artifacts after every validation succeeds\")\n")
  (let (prefix-rule seventy-enabled seventy-disabled sixty-enabled)
    (let ((current-prefix-arg 32))
      (call-interactively #'column-enforce-n))
    (setq prefix-rule (neomacs-cem-test-state))
    (70-column-rule)
    (setq seventy-enabled (neomacs-cem-test-state))
    (70-column-rule)
    (setq seventy-disabled (neomacs-cem-test-state))
    (60-column-rule)
    (setq sixty-enabled (neomacs-cem-test-state))
    (column-enforce-mode -1)
    (list :prefix-rule prefix-rule
          :seventy-enabled seventy-enabled
          :seventy-disabled seventy-disabled
          :sixty-enabled sixty-enabled
          :final (neomacs-cem-test-state)
          :column-local (local-variable-p 'column-enforce-column)
          :text (buffer-substring-no-properties (point-min) (point-max)))))
"####;
    let expected = expect![[
        r#"OK (:prefix-rule (:mode t :column 32 :lighter " 32col" :jit-registered t :overlays ((:line 1 :start 33 :end 70 :start-column 32 :end-column 69 :text "cts after every validation succeeds\")" :face column-enforce-face :marker t))) :seventy-enabled (:mode t :column 70 :lighter " 70col" :jit-registered t :overlays nil) :seventy-disabled (:mode nil :column 70 :lighter " 70col" :jit-registered nil :overlays nil) :sixty-enabled (:mode t :column 60 :lighter " 60col" :jit-registered t :overlays ((:line 1 :start 61 :end 70 :start-column 60 :end-column 69 :text "ucceeds\")" :face column-enforce-face :marker t))) :final (:mode nil :column 60 :lighter " 60col" :jit-registered nil :overlays nil) :column-local t :text "(message \"publish release artifacts after every validation succeeds\")\n")"#
    ]];
    ParityBatchCase::value(
        "interactive_and_predefined_rules_drive_the_review_lifecycle",
        elisp_form,
        expected,
    )
}

fn global_mode_selects_programming_buffers_and_honors_a_team_policy() -> ParityBatchCase {
    let elisp_form = r####"
(let ((source (generate-new-buffer " *cem-source*"))
      (notes (generate-new-buffer " *cem-notes*"))
      (generated (generate-new-buffer " *cem-generated*"))
      (later-source (generate-new-buffer " *cem-later-source*")))
  (unwind-protect
      (progn
        (global-column-enforce-mode -1)
        (with-current-buffer source (emacs-lisp-mode))
        (with-current-buffer notes (text-mode))
        (with-current-buffer generated (emacs-lisp-mode))
        (global-column-enforce-mode 1)
        (let ((default-policy
               (list :global global-column-enforce-mode
                     :source
                     (buffer-local-value 'column-enforce-mode source)
                     :notes
                     (buffer-local-value 'column-enforce-mode notes)
                     :generated
                     (buffer-local-value 'column-enforce-mode generated))))
          (global-column-enforce-mode -1)
          (setq column-enforce-should-enable-p
                (lambda ()
                  (and (derived-mode-p 'prog-mode)
                       (not (string-match-p "generated" (buffer-name))))))
          (global-column-enforce-mode 1)
          (with-current-buffer later-source (emacs-lisp-mode))
          (let ((team-policy
                 (list :global global-column-enforce-mode
                       :source
                       (buffer-local-value 'column-enforce-mode source)
                       :notes
                       (buffer-local-value 'column-enforce-mode notes)
                       :generated
                       (buffer-local-value 'column-enforce-mode generated)
                       :later-source
                       (buffer-local-value 'column-enforce-mode later-source))))
            (global-column-enforce-mode -1)
            (list
             :default-policy default-policy
             :team-policy team-policy
             :disabled
             (list :global global-column-enforce-mode
                   :source
                   (buffer-local-value 'column-enforce-mode source)
                   :notes
                   (buffer-local-value 'column-enforce-mode notes)
                   :generated
                   (buffer-local-value 'column-enforce-mode generated)
                   :later-source
                   (buffer-local-value 'column-enforce-mode later-source))))))
    (global-column-enforce-mode -1)
    (setq column-enforce-should-enable-p nil)
    (dolist (buffer (list source notes generated later-source))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))
"####;
    let expected = expect![
        "OK (:default-policy (:global t :source t :notes nil :generated t) :team-policy (:global t :source t :notes nil :generated nil :later-source t) :disabled (:global nil :source nil :notes nil :generated nil :later-source nil))"
    ];
    ParityBatchCase::value(
        "global_mode_selects_programming_buffers_and_honors_a_team_policy",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn column_enforce_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COLUMN_ENFORCE_MODE_MELPA_PIN, "column-enforce-mode.el")
        .expect("prepare pinned Column Enforce Mode source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn column_enforce_mode_practical_workflows_batch() {
    let cases = vec![
        marks_real_code_by_display_column_across_tabs_and_wide_characters(),
        applies_the_comment_policy_without_exempting_strings_or_code(),
        updates_warnings_after_a_developer_shortens_and_extends_a_line(),
        warning_boundaries_follow_insertions_like_live_editor_annotations(),
        uses_contextual_limits_for_headers_continuations_and_generated_lines(),
        interactive_and_predefined_rules_drive_the_review_lifecycle(),
        global_mode_selects_programming_buffers_and_honors_a_team_policy(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("column-enforce-mode parity batch");
    assert_oracle_batch_cases(
        column_enforce_mode_oracle(),
        test_name,
        "column-enforce-mode parity",
        &cases,
    );
}
