use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, VI_TILDE_FRINGE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'fringe)
(require 'vi-tilde-fringe)

(global-vi-tilde-fringe-mode -1)
(vi-tilde-fringe-mode -1)

(defun neomacs-vi-tilde-fringe-test-state ()
  (list :mode vi-tilde-fringe-mode
        :indicate indicate-empty-lines
        :indicate-local (local-variable-p 'indicate-empty-lines)
        :indicators (copy-tree fringe-indicator-alist)
        :indicators-local (local-variable-p 'fringe-indicator-alist)
        :tilde-count
        (cl-count '(empty-line . vi-tilde-fringe-bitmap)
                  fringe-indicator-alist :test #'equal)))

(defun neomacs-vi-tilde-fringe-test-capture (function)
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"####;

fn exact_package_customization_face_and_commands_match() -> ParityBatchCase {
    let elisp_form = r####"
(let ((descriptor (cadr (assq 'vi-tilde-fringe package-alist))))
  (list :package (package-desc-name descriptor)
        :version (package-version-join (package-desc-version descriptor))
        :requirements (package-desc-reqs descriptor)
        :feature (featurep 'vi-tilde-fringe)
        :local-command (commandp 'vi-tilde-fringe-mode)
        :global-command (commandp 'global-vi-tilde-fringe-mode)
        :bitmap (append vi-tilde-fringe-bitmap-array nil)
        :bitmap-custom-type
        (get 'vi-tilde-fringe-bitmap-array 'custom-type)
        :face-inherit
        (face-attribute 'vi-tilde-fringe-face :inherit nil 'default)
        :group (get 'vi-tilde-fringe-bitmap-array 'custom-group)
        :lighter (cadr (assq 'vi-tilde-fringe-mode minor-mode-alist))))
"####;
    let expected = expect![[
        r#"OK (:package vi-tilde-fringe :version "20141028.242" :requirements ((emacs (24))) :feature t :local-command t :global-command t :bitmap (0 0 0 113 219 142 0 0) :bitmap-custom-type sexp :face-inherit 'default :group nil :lighter " ~")"#
    ]];
    ParityBatchCase::value(
        "exact_package_customization_face_and_commands_match",
        elisp_form,
        expected,
    )
}

fn local_mode_registers_the_bitmap_and_maps_empty_rows_without_destroying_configuration_on_exit()
-> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let ((before (neomacs-vi-tilde-fringe-test-state)))
    (vi-tilde-fringe-mode 1)
    (let ((enabled (neomacs-vi-tilde-fringe-test-state))
          (bitmap
           (list :index (get 'vi-tilde-fringe-bitmap 'fringe)
                 :predicate (and (fringe-bitmap-p
                                  'vi-tilde-fringe-bitmap)
                                 t)
                 :listed (and (memq 'vi-tilde-fringe-bitmap
                                    fringe-bitmaps)
                              t))))
      (vi-tilde-fringe-mode -1)
      (list :before before
            :enabled enabled
            :bitmap bitmap
            :disabled (neomacs-vi-tilde-fringe-test-state)
            :bitmap-after-disable
            (get 'vi-tilde-fringe-bitmap 'fringe)))))
"####;
    let expected = expect![
        "OK (:before (:mode nil :indicate nil :indicate-local nil :indicators ((truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local nil :tilde-count 0) :enabled (:mode t :indicate t :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :bitmap (:index 25 :predicate t :listed t) :disabled (:mode nil :indicate nil :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :bitmap-after-disable 25)"
    ];
    ParityBatchCase::value(
        "local_mode_registers_the_bitmap_and_maps_empty_rows_without_destroying_configuration_on_exit",
        elisp_form,
        expected,
    )
}

fn repeated_activation_is_idempotent_and_keeps_preexisting_indicator_fallbacks() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (setq-local fringe-indicator-alist
              '((empty-line . release-empty-line)
                (unknown . release-unknown)))
  (vi-tilde-fringe-mode 1)
  (let ((first (neomacs-vi-tilde-fringe-test-state)))
    (vi-tilde-fringe-mode 1)
    (let ((second (neomacs-vi-tilde-fringe-test-state)))
      (vi-tilde-fringe-mode -1)
      (list :first first
            :second second
            :disabled (neomacs-vi-tilde-fringe-test-state)))))
"####;
    let expected = expect![
        "OK (:first (:mode t :indicate t :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (empty-line . release-empty-line) (unknown . release-unknown)) :indicators-local t :tilde-count 1) :second (:mode t :indicate t :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (empty-line . release-empty-line) (unknown . release-unknown)) :indicators-local t :tilde-count 1) :disabled (:mode nil :indicate nil :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (empty-line . release-empty-line) (unknown . release-unknown)) :indicators-local t :tilde-count 1))"
    ];
    ParityBatchCase::value(
        "repeated_activation_is_idempotent_and_keeps_preexisting_indicator_fallbacks",
        elisp_form,
        expected,
    )
}

fn custom_bitmap_rows_are_forwarded_to_the_real_registry_before_mode_hooks_run() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((original (symbol-function 'define-fringe-bitmap))
      calls hook-states)
  (with-temp-buffer
    (let ((vi-tilde-fringe-bitmap-array
           [#b10000001 #b01000010 #b00100100 #b00011000])
          (vi-tilde-fringe-mode-hook
           (list (lambda ()
                   (push (list :mode vi-tilde-fringe-mode
                               :indicate indicate-empty-lines
                               :mapping
                               (copy-tree (car fringe-indicator-alist))
                               :bitmap-index
                               (get 'vi-tilde-fringe-bitmap 'fringe))
                         hook-states)))))
      (cl-letf (((symbol-function 'define-fringe-bitmap)
                 (lambda (&rest args)
                   (push (copy-tree args) calls)
                   (apply original args))))
        (vi-tilde-fringe-mode 1)
        (let ((enabled (neomacs-vi-tilde-fringe-test-state)))
          (vi-tilde-fringe-mode -1)
          (list :define-calls (nreverse calls)
                :hook-states (nreverse hook-states)
                :enabled enabled
                :disabled (neomacs-vi-tilde-fringe-test-state)
                :bitmap-index
                (get 'vi-tilde-fringe-bitmap 'fringe)))))))
"####;
    let expected = expect![
        "OK (:define-calls ((vi-tilde-fringe-bitmap [129 66 36 24] nil nil center)) :hook-states ((:mode t :indicate t :mapping (empty-line . vi-tilde-fringe-bitmap) :bitmap-index 25) (:mode nil :indicate nil :mapping (empty-line . vi-tilde-fringe-bitmap) :bitmap-index 25)) :enabled (:mode t :indicate t :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :disabled (:mode nil :indicate nil :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :bitmap-index 25)"
    ];
    ParityBatchCase::value(
        "custom_bitmap_rows_are_forwarded_to_the_real_registry_before_mode_hooks_run",
        elisp_form,
        expected,
    )
}

fn local_activation_and_deactivation_do_not_change_another_buffers_fringe_policy() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((editor (generate-new-buffer " *vi-tilde-editor*"))
      (dashboard (generate-new-buffer " *vi-tilde-dashboard*")))
  (unwind-protect
      (progn
        (with-current-buffer dashboard
          (setq-local indicate-empty-lines 'right)
          (setq-local fringe-indicator-alist
                      '((empty-line . dashboard-empty-line))))
        (with-current-buffer editor
          (vi-tilde-fringe-mode 1))
        (let ((enabled
               (list :editor
                     (with-current-buffer editor
                       (neomacs-vi-tilde-fringe-test-state))
                     :dashboard
                     (with-current-buffer dashboard
                       (neomacs-vi-tilde-fringe-test-state)))))
          (with-current-buffer editor
            (vi-tilde-fringe-mode -1))
          (list :enabled enabled
                :disabled
                (list :editor
                      (with-current-buffer editor
                        (neomacs-vi-tilde-fringe-test-state))
                      :dashboard
                      (with-current-buffer dashboard
                        (neomacs-vi-tilde-fringe-test-state))))))
    (kill-buffer editor)
    (kill-buffer dashboard)))
"####;
    let expected = expect![
        "OK (:enabled (:editor (:mode t :indicate t :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :dashboard (:mode nil :indicate right :indicate-local t :indicators ((empty-line . dashboard-empty-line)) :indicators-local t :tilde-count 0)) :disabled (:editor (:mode nil :indicate nil :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :dashboard (:mode nil :indicate right :indicate-local t :indicators ((empty-line . dashboard-empty-line)) :indicators-local t :tilde-count 0)))"
    ];
    ParityBatchCase::value(
        "local_activation_and_deactivation_do_not_change_another_buffers_fringe_policy",
        elisp_form,
        expected,
    )
}

fn invalid_bitmap_customization_reports_the_primitive_error_and_preserves_partial_mode_state()
-> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let ((vi-tilde-fringe-bitmap-array '(128 64 32))
        result)
    (unwind-protect
        (setq result
              (let ((outcome
                     (neomacs-vi-tilde-fringe-test-capture
                      (lambda () (vi-tilde-fringe-mode 1)))))
                (list :outcome outcome
                      :state (neomacs-vi-tilde-fringe-test-state)
                      :bitmap-still-defined
                      (and (fringe-bitmap-p 'vi-tilde-fringe-bitmap) t))))
      (vi-tilde-fringe-mode -1))
    result))
"####;
    let expected = expect![[
        r#"OK (:outcome (:error wrong-type-argument :data (arrayp (128 64 32)) :message "Wrong type argument: arrayp, (128 64 32)") :state (:mode t :indicate nil :indicate-local nil :indicators ((truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local nil :tilde-count 0) :bitmap-still-defined t)"#
    ]];
    ParityBatchCase::value(
        "invalid_bitmap_customization_reports_the_primitive_error_and_preserves_partial_mode_state",
        elisp_form,
        expected,
    )
}

fn global_mode_updates_existing_and_future_buffers_but_skips_the_minibuffer() -> ParityBatchCase {
    let elisp_form = r####"
(let ((editor (generate-new-buffer " *vi-tilde-global-editor*"))
      (notes (generate-new-buffer " *vi-tilde-global-notes*"))
      (future nil)
      (minibuffer-buffer (window-buffer (minibuffer-window))))
  (unwind-protect
      (progn
        (with-current-buffer editor (emacs-lisp-mode))
        (with-current-buffer notes (text-mode))
        (global-vi-tilde-fringe-mode 1)
        (setq future (generate-new-buffer " *vi-tilde-global-future*"))
        (with-current-buffer future (fundamental-mode))
        (let ((enabled
               (list
                :global global-vi-tilde-fringe-mode
                :hook
                (and (memq
                      'global-vi-tilde-fringe-mode-enable-in-buffer
                      after-change-major-mode-hook)
                     t)
                :editor (with-current-buffer editor
                          (neomacs-vi-tilde-fringe-test-state))
                :notes (with-current-buffer notes
                         (neomacs-vi-tilde-fringe-test-state))
                :future (with-current-buffer future
                          (neomacs-vi-tilde-fringe-test-state))
                :minibuffer
                (with-current-buffer minibuffer-buffer
                  (neomacs-vi-tilde-fringe-test-state)))))
          (global-vi-tilde-fringe-mode -1)
          (list
           :enabled enabled
           :disabled
           (list
            :global global-vi-tilde-fringe-mode
            :hook
            (and (memq
                  'global-vi-tilde-fringe-mode-enable-in-buffer
                  after-change-major-mode-hook)
                 t)
            :editor (with-current-buffer editor
                      (neomacs-vi-tilde-fringe-test-state))
            :notes (with-current-buffer notes
                     (neomacs-vi-tilde-fringe-test-state))
            :future (with-current-buffer future
                      (neomacs-vi-tilde-fringe-test-state))
            :minibuffer
            (with-current-buffer minibuffer-buffer
              (neomacs-vi-tilde-fringe-test-state))))))
    (global-vi-tilde-fringe-mode -1)
    (kill-buffer editor)
    (kill-buffer notes)
    (when (buffer-live-p future) (kill-buffer future))))
"####;
    let expected = expect![
        "OK (:enabled (:global t :hook t :editor (:mode t :indicate t :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :notes (:mode t :indicate t :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :future (:mode t :indicate t :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :minibuffer (:mode nil :indicate nil :indicate-local nil :indicators ((truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local nil :tilde-count 0)) :disabled (:global nil :hook nil :editor (:mode nil :indicate nil :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :notes (:mode nil :indicate nil :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :future (:mode nil :indicate nil :indicate-local t :indicators ((empty-line . vi-tilde-fringe-bitmap) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local t :tilde-count 1) :minibuffer (:mode nil :indicate nil :indicate-local nil :indicators ((truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) :indicators-local nil :tilde-count 0)))"
    ];
    ParityBatchCase::value(
        "global_mode_updates_existing_and_future_buffers_but_skips_the_minibuffer",
        elisp_form,
        expected,
    )
    .fresh_process()
}

#[test]
fn vi_tilde_fringe_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(VI_TILDE_FRINGE_MELPA_PIN, "vi-tilde-fringe.el")
            .expect("prepare revision-pinned Vi Tilde Fringe source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "vi-tilde-fringe-package-batch",
        "Vi Tilde Fringe",
        &[
            exact_package_customization_face_and_commands_match(),
            local_mode_registers_the_bitmap_and_maps_empty_rows_without_destroying_configuration_on_exit(),
            repeated_activation_is_idempotent_and_keeps_preexisting_indicator_fallbacks(),
            custom_bitmap_rows_are_forwarded_to_the_real_registry_before_mode_hooks_run(),
            local_activation_and_deactivation_do_not_change_another_buffers_fringe_policy(),
            invalid_bitmap_customization_reports_the_primitive_error_and_preserves_partial_mode_state(),
            global_mode_updates_existing_and_future_buffers_but_skips_the_minibuffer(),
        ],
    );
}
