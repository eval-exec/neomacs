use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, WRITEROOM_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'writeroom-mode)

(defvar neomacs-writeroom-test-events nil)

(defun neomacs-writeroom-test-global-effect (argument)
  "Record a global Writeroom effect ARGUMENT."
  (push (list :global argument) neomacs-writeroom-test-events))

(defun neomacs-writeroom-test-local-effect (argument)
  "Record a buffer-local Writeroom effect ARGUMENT."
  (push (list :local argument) neomacs-writeroom-test-events))

(defun neomacs-writeroom-test-enable-hook ()
  "Record the post-enable hook."
  (push 'enable-hook neomacs-writeroom-test-events))

(defun neomacs-writeroom-test-disable-hook ()
  "Record the post-disable hook."
  (push 'disable-hook neomacs-writeroom-test-events))

(define-derived-mode neomacs-writeroom-test-notes-mode text-mode "Parity Notes")

(defun neomacs-writeroom-test-state ()
  "Return stable focused-writing state for the current buffer."
  (list :writeroom (and writeroom-mode t)
        :visual-fill (and visual-fill-column-mode t)
        :width visual-fill-column-width
        :center visual-fill-column-center-text
        :outside visual-fill-column-fringes-outside-margins
        :extra-width visual-fill-column-extra-text-width
        :mode-line mode-line-format
        :header-line header-line-format
        :line-spacing line-spacing
        :buffers (length writeroom--buffers)
        :frame-live (and writeroom--frame
                         (frame-live-p writeroom--frame)
                         t)))
"###;

fn package_defaults_and_keymap_describe_the_focused_writing_policy() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'writeroom-mode package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :features
         (mapcar (lambda (feature) (and (featurep feature) t))
                 '(writeroom-mode visual-fill-column)))
   :layout
   (list :width writeroom-width
         :added-left
         (if (functionp writeroom-added-width-left)
             'function
           writeroom-added-width-left)
         :mode-line writeroom-mode-line
         :header-line writeroom-header-line
         :toggle-position writeroom-mode-line-toggle-position
         :maximize writeroom-maximize-window
         :restore writeroom-restore-window-config
         :spacing writeroom-extra-line-spacing
         :outside writeroom-fringes-outside-margins)
   :admission
   (list writeroom-major-modes writeroom-use-derived-modes
         writeroom-major-modes-exceptions)
   :binding (lookup-key writeroom-mode-map (kbd "s-?"))))
"###;
    let expected = expect![[
        r#"OK (:package (:name writeroom-mode :version "20250204.2335" :requirements ((emacs (25 1)) (visual-fill-column (2 2))) :features (t t)) :layout (:width 80 :added-left function :mode-line nil :header-line nil :toggle-position header-line-format :maximize t :restore nil :spacing nil :outside t) :admission ((text-mode) t nil) :binding writeroom-toggle-mode-line)"#
    ]];
    ParityBatchCase::value(
        "package_defaults_and_keymap_describe_the_focused_writing_policy",
        elisp_form,
        expected,
    )
}

fn enable_disable_roundtrip_applies_focus_settings_hooks_and_restores_local_state()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((writeroom--buffers nil)
      (writeroom--frame nil)
      (writeroom-global-effects '(neomacs-writeroom-test-global-effect))
      (writeroom-local-effects '(neomacs-writeroom-test-local-effect))
      (writeroom-mode-enable-hook '(neomacs-writeroom-test-enable-hook))
      (writeroom-mode-disable-hook '(neomacs-writeroom-test-disable-hook))
      (writeroom-maximize-window nil)
      (writeroom-width 72)
      (writeroom-added-width-left 3)
      (writeroom-mode-line nil)
      (writeroom-header-line '("Focused"))
      (writeroom-extra-line-spacing 0.4)
      (neomacs-writeroom-test-events nil))
  (with-temp-buffer
    (text-mode)
    (setq-local mode-line-format '("Original mode"))
    (setq-local header-line-format '("Original header"))
    (setq-local line-spacing 2)
    (let ((before (neomacs-writeroom-test-state)))
      (writeroom-mode 1)
      (let ((enabled (neomacs-writeroom-test-state))
            (saved writeroom--saved-data))
        (writeroom-mode -1)
        (list :before before
              :enabled enabled
              :saved saved
              :disabled (neomacs-writeroom-test-state)
              :events (nreverse neomacs-writeroom-test-events)
              :locals
              (mapcar #'local-variable-p
                      '(mode-line-format header-line-format line-spacing
                        visual-fill-column-width
                        visual-fill-column-center-text
                        visual-fill-column-extra-text-width)))))))
"###;
    let expected = expect![[
        r#"OK (:before (:writeroom nil :visual-fill nil :width nil :center nil :outside t :extra-width nil :mode-line #1=("Original mode") :header-line #2=("Original header") :line-spacing 2 :buffers 0 :frame-live nil) :enabled (:writeroom t :visual-fill t :width 72 :center t :outside t :extra-width (3 . 0) :mode-line nil :header-line ("Focused") :line-spacing 0.4 :buffers 1 :frame-live t) :saved ((mode-line-format . #1#) (header-line-format . #2#) (line-spacing . 2)) :disabled (:writeroom nil :visual-fill nil :width nil :center nil :outside t :extra-width nil :mode-line #1# :header-line #2# :line-spacing 2 :buffers 0 :frame-live nil) :events ((:global 1) (:local 1) enable-hook (:global -1) (:local -1) disable-hook) :locals (t t t nil nil nil))"#
    ]];
    ParityBatchCase::value(
        "enable_disable_roundtrip_applies_focus_settings_hooks_and_restores_local_state",
        elisp_form,
        expected,
    )
}

fn preexisting_visual_fill_mode_is_reenabled_after_the_focus_session() -> ParityBatchCase {
    let elisp_form = r###"
(let ((writeroom--buffers nil)
      (writeroom--frame nil)
      (writeroom-global-effects nil)
      (writeroom-maximize-window nil)
      (writeroom-width 74)
      (writeroom-added-width-left 5))
  (with-temp-buffer
    (text-mode)
    (setq-local visual-fill-column-width 56)
    (setq-local visual-fill-column-center-text nil)
    (setq-local visual-fill-column-extra-text-width '(2 . 1))
    (visual-fill-column-mode 1)
    (let ((before (neomacs-writeroom-test-state)))
      (writeroom-mode 1)
      (let ((focused (neomacs-writeroom-test-state)))
        (writeroom-mode -1)
        (list :before before
              :focused focused
              :restored (neomacs-writeroom-test-state)
              :saved-visual writeroom--saved-visual-fill-column)))))
"###;
    let expected = expect![[
        r#"OK (:before (:writeroom nil :visual-fill t :width 56 :center nil :outside t :extra-width (2 . 1) :mode-line #1=("%e" mode-line-front-space (:propertize ("" mode-line-mule-info mode-line-client mode-line-modified mode-line-remote mode-line-window-dedicated) display (min-width (6.0))) mode-line-frame-identification mode-line-buffer-identification "   " mode-line-position (project-mode-line project-mode-line-format) (vc-mode vc-mode) "  " mode-line-modes mode-line-misc-info mode-line-end-spaces) :header-line nil :line-spacing nil :buffers 0 :frame-live nil) :focused (:writeroom t :visual-fill t :width 74 :center t :outside t :extra-width (5 . 0) :mode-line nil :header-line nil :line-spacing nil :buffers 1 :frame-live t) :restored (:writeroom nil :visual-fill t :width nil :center nil :outside t :extra-width nil :mode-line #1# :header-line nil :line-spacing nil :buffers 0 :frame-live nil) :saved-visual t)"#
    ]];
    ParityBatchCase::value(
        "preexisting_visual_fill_mode_is_reenabled_after_the_focus_session",
        elisp_form,
        expected,
    )
}

fn live_width_controls_adjust_and_reset_the_centered_writing_area() -> ParityBatchCase {
    let elisp_form = r###"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *writeroom-width*"))
        (writeroom-width 0.5))
    (unwind-protect
        (progn
          (set-window-buffer (selected-window) buffer)
          (with-current-buffer buffer
            (text-mode)
            (setq-local visual-fill-column-width 24)
            (setq-local visual-fill-column-center-text t)
            (visual-fill-column-mode 1)
            (let ((window-width (window-total-width))
                  (calculated (writeroom--calculate-width)))
              (writeroom-adjust-width 7)
              (let ((after-adjust visual-fill-column-width))
                (writeroom-increase-width)
                (let ((after-increase visual-fill-column-width))
                  (writeroom-decrease-width)
                  (let ((after-decrease visual-fill-column-width))
                    (writeroom-adjust-width nil)
                    (list :window-width window-width
                          :calculated calculated
                          :widths (list after-adjust after-increase
                                        after-decrease visual-fill-column-width)
                          :margins (window-margins (selected-window)))))))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"###;
    let expected =
        expect!["OK (:window-width 80 :calculated 40 :widths (31 33 31 40) :margins (20 . 20))"];
    ParityBatchCase::value(
        "live_width_controls_adjust_and_reset_the_centered_writing_area",
        elisp_form,
        expected,
    )
}

fn hidden_mode_line_can_be_temporarily_revealed_in_header_or_mode_line() -> ParityBatchCase {
    let elisp_form = r###"
(let ((writeroom--buffers nil)
      (writeroom--frame nil)
      (writeroom-global-effects nil)
      (writeroom-maximize-window nil)
      (writeroom-mode-line nil)
      (writeroom-header-line nil))
  (with-temp-buffer
    (text-mode)
    (setq-local mode-line-format '("Original mode" mode-line-buffer-identification))
    (setq-local header-line-format '("Original header"))
    (writeroom-mode 1)
    (let ((hidden (list mode-line-format header-line-format
                        writeroom--mode-line-showing)))
      (let ((writeroom-mode-line-toggle-position 'header-line-format))
        (writeroom-toggle-mode-line)
        (let ((header-shown (list mode-line-format header-line-format
                                  writeroom--mode-line-showing)))
          (writeroom-toggle-mode-line)
          (let ((header-hidden (list mode-line-format header-line-format
                                     writeroom--mode-line-showing))
                (writeroom-mode-line-toggle-position 'mode-line-format))
            (writeroom-toggle-mode-line)
            (let ((mode-shown (list mode-line-format header-line-format
                                    writeroom--mode-line-showing)))
              (writeroom-toggle-mode-line)
              (let ((mode-hidden (list mode-line-format header-line-format
                                       writeroom--mode-line-showing)))
                (writeroom-mode -1)
                (list :hidden hidden
                      :header-shown header-shown
                      :header-hidden header-hidden
                      :mode-shown mode-shown
                      :mode-hidden mode-hidden
                      :restored (list mode-line-format header-line-format))))))))))
"###;
    let expected = expect![[
        r#"OK (:hidden (nil nil nil) :header-shown (nil #1=("Original mode" mode-line-buffer-identification) t) :header-hidden (nil nil nil) :mode-shown (#1# nil t) :mode-hidden (nil nil nil) :restored (#1# ("Original header")))"#
    ]];
    ParityBatchCase::value(
        "hidden_mode_line_can_be_temporarily_revealed_in_header_or_mode_line",
        elisp_form,
        expected,
    )
}

fn global_effects_span_the_first_enable_until_the_last_buffer_is_killed() -> ParityBatchCase {
    let elisp_form = r###"
(let ((writeroom--buffers nil)
      (writeroom--frame nil)
      (writeroom-global-effects '(neomacs-writeroom-test-global-effect))
      (writeroom-local-effects nil)
      (writeroom-maximize-window nil)
      (neomacs-writeroom-test-events nil)
      (first (generate-new-buffer " *writeroom-first*"))
      (second (generate-new-buffer " *writeroom-second*")))
  (unwind-protect
      (progn
        (with-current-buffer first
          (text-mode)
          (writeroom-mode 1))
        (let ((after-first
               (list (length writeroom--buffers)
                     (reverse (copy-sequence neomacs-writeroom-test-events)))))
          (with-current-buffer second
            (text-mode)
            (writeroom-mode 1))
          (let ((after-second
                 (list (length writeroom--buffers)
                       (reverse (copy-sequence neomacs-writeroom-test-events)))))
            (with-current-buffer first
              (writeroom-mode -1))
            (let ((after-first-disabled
                   (list (length writeroom--buffers)
                         (reverse (copy-sequence neomacs-writeroom-test-events)))))
              (kill-buffer second)
              (list :after-first after-first
                    :after-second after-second
                    :after-first-disabled after-first-disabled
                    :after-last-killed
                    (list (length writeroom--buffers)
                          writeroom--frame
                          (nreverse neomacs-writeroom-test-events)))))))
    (when (buffer-live-p first) (kill-buffer first))
    (when (buffer-live-p second) (kill-buffer second))))
"###;
    let expected = expect![
        "OK (:after-first (1 (#1=(:global 1))) :after-second (2 (#1#)) :after-first-disabled (1 (#1#)) :after-last-killed (0 nil (#1# (:global -1))))"
    ];
    ParityBatchCase::value(
        "global_effects_span_the_first_enable_until_the_last_buffer_is_killed",
        elisp_form,
        expected,
    )
}

fn global_admission_supports_exact_derived_regex_and_exception_policies() -> ParityBatchCase {
    let elisp_form = r###"
(let ((writeroom--buffers nil)
      (writeroom--frame nil)
      (writeroom-global-effects nil)
      (writeroom-maximize-window nil))
  (cl-labels
      ((probe (mode allowed derived exceptions)
         (with-temp-buffer
           (funcall mode)
           (let ((writeroom-major-modes allowed)
                 (writeroom-use-derived-modes derived)
                 (writeroom-major-modes-exceptions exceptions))
             (turn-on-writeroom-mode)
             (prog1 (list major-mode (and writeroom-mode t))
               (when writeroom-mode (writeroom-mode -1)))))))
    (list
     :exact (probe #'text-mode '(text-mode) t nil)
     :derived (probe #'neomacs-writeroom-test-notes-mode '(text-mode) t nil)
     :derived-disabled
     (probe #'neomacs-writeroom-test-notes-mode '(text-mode) nil nil)
     :regexp (probe #'emacs-lisp-mode '("lisp-mode") t nil)
     :exception
     (probe #'neomacs-writeroom-test-notes-mode
            '(text-mode) t '(neomacs-writeroom-test-notes-mode))
     :unlisted (probe #'special-mode '(text-mode) t nil))))
"###;
    let expected = expect![
        "OK (:exact (text-mode t) :derived (neomacs-writeroom-test-notes-mode t) :derived-disabled (neomacs-writeroom-test-notes-mode nil) :regexp (emacs-lisp-mode t) :exception (neomacs-writeroom-test-notes-mode nil) :unlisted (special-mode nil))"
    ];
    ParityBatchCase::value(
        "global_admission_supports_exact_derived_regex_and_exception_policies",
        elisp_form,
        expected,
    )
}

fn custom_frame_effect_and_window_maximization_restore_the_original_workspace() -> ParityBatchCase {
    let elisp_form = r###"
(save-window-excursion
  (delete-other-windows)
  (let* ((frame (selected-frame))
         (original-layout (frame-parameter frame 'neomacs-layout))
         (primary (selected-window))
         (secondary (split-window-right))
         (buffer (generate-new-buffer " *writeroom-layout*"))
         (writeroom--buffers nil)
         (writeroom--frame frame)
         (writeroom-global-effects nil)
         (writeroom-maximize-window t)
         (writeroom-restore-window-config t))
    (unwind-protect
        (progn
          (define-writeroom-global-effect neomacs-layout 'focused)
          (set-frame-parameter frame 'neomacs-layout 'normal)
          (writeroom-set-neomacs-layout 1)
          (let ((effect-on
                 (list (frame-parameter frame 'neomacs-layout)
                       (frame-parameter frame 'writeroom-neomacs-layout))))
            (writeroom-set-neomacs-layout -1)
            (set-window-buffer primary buffer)
            (with-current-buffer buffer
              (text-mode)
              (let ((before (length (window-list))))
                (writeroom-mode 1)
                (let ((focused (length (window-list))))
                  (writeroom-mode -1)
                  (list :effect-on effect-on
                        :effect-off
                        (list (frame-parameter frame 'neomacs-layout)
                              (frame-parameter frame 'writeroom-neomacs-layout))
                        :windows (list before focused (length (window-list)))
                        :secondary-live (and (window-live-p secondary) t)))))))
      (set-frame-parameter frame 'neomacs-layout original-layout)
      (set-frame-parameter frame 'writeroom-neomacs-layout nil)
      (fmakunbound 'writeroom-set-neomacs-layout)
      (when (buffer-live-p buffer) (kill-buffer buffer)))))
"###;
    let expected = expect![
        "OK (:effect-on (focused normal) :effect-off (normal nil) :windows (2 1 2) :secondary-live t)"
    ];
    ParityBatchCase::value(
        "custom_frame_effect_and_window_maximization_restore_the_original_workspace",
        elisp_form,
        expected,
    )
}

#[test]
fn writeroom_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(WRITEROOM_MODE_MELPA_PIN, "writeroom-mode.el")
            .expect("prepare revision-pinned Writeroom Mode below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "writeroom-mode-package-batch",
        "Writeroom Mode",
        &[
            package_defaults_and_keymap_describe_the_focused_writing_policy(),
            enable_disable_roundtrip_applies_focus_settings_hooks_and_restores_local_state(),
            preexisting_visual_fill_mode_is_reenabled_after_the_focus_session(),
            live_width_controls_adjust_and_reset_the_centered_writing_area(),
            hidden_mode_line_can_be_temporarily_revealed_in_header_or_mode_line(),
            global_effects_span_the_first_enable_until_the_last_buffer_is_killed(),
            global_admission_supports_exact_derived_regex_and_exception_policies(),
            custom_frame_effect_and_window_maximization_restore_the_original_workspace(),
        ],
    );
}
