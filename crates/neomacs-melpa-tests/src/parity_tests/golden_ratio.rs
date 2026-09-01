use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GOLDEN_RATIO_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'golden-ratio)

(defun neomacs-golden-ratio-test-windows ()
  (mapcar
   (lambda (window)
     (list :buffer (buffer-name (window-buffer window))
           :selected (eq window (selected-window))
           :width (window-width window)
           :height (window-height window)
           :margins (window-margins window)))
   (window-list nil 'nomini)))

(defun neomacs-golden-ratio-test-sizes ()
  (mapcar
   (lambda (window)
     (list (buffer-name (window-buffer window))
           (window-width window) (window-height window)))
   (window-list nil 'nomini)))

(defun neomacs-golden-ratio-test-layout (function)
  (save-window-excursion
    (delete-other-windows)
    (let ((buffers (mapcar #'generate-new-buffer
                           '("*golden-main*" "*golden-right*" "*golden-lower*"))))
      (unwind-protect
          (progn
            (set-window-buffer (selected-window) (nth 0 buffers))
            (let ((right (split-window-right)))
              (set-window-buffer right (nth 1 buffers))
              (select-window right)
              (let ((lower (split-window-below)))
                (set-window-buffer lower (nth 2 buffers))))
            (funcall function buffers))
        (dolist (buffer buffers)
          (when (buffer-live-p buffer)
            (with-current-buffer buffer (set-buffer-modified-p nil))
            (kill-buffer buffer)))))))
"####;

fn scaling_controls_compute_editing_dimensions_for_real_frame_shapes() -> ParityBatchCase {
    let elisp_form = r####"
(cl-labels
    ((dimensions (width height adjust auto maximum)
       (cl-letf (((symbol-function 'frame-width) (lambda (&optional _frame) width))
                 ((symbol-function 'frame-height) (lambda (&optional _frame) height)))
         (let ((golden-ratio-adjust-factor adjust)
               (golden-ratio-auto-scale auto)
               (golden-ratio-max-width maximum))
           (list :scale (golden-ratio--scale-factor)
                 :dimensions (golden-ratio--dimensions))))))
  (list :standard (dimensions 100 62 1.0 nil nil)
        :wide (dimensions 220 80 0.65 nil nil)
        :automatic (dimensions 220 80 1.0 t nil)
        :capped (dimensions 220 80 1.0 nil 72)))
"####;
    let expected = expect![
        "OK (:standard (:scale 1.0 :dimensions (38 61)) :wide (:scale 0.65 :dimensions (49 88)) :automatic (:scale 0.784 :dimensions (49 106)) :capped (:scale 1.0 :dimensions (49 72)))"
    ];
    ParityBatchCase::value(
        "scaling_controls_compute_editing_dimensions_for_real_frame_shapes",
        elisp_form,
        expected,
    )
}

fn selecting_each_pane_rebalances_and_enlarges_the_active_editor() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-golden-ratio-test-layout
 (lambda (_buffers)
   (let ((golden-ratio-mode t)
         snapshots)
     (dolist (window (window-list nil 'nomini))
       (select-window window)
       (golden-ratio)
       (push (neomacs-golden-ratio-test-windows) snapshots))
     (nreverse snapshots))))
"####;
    let expected = expect![[
        r#"OK (((:buffer "*golden-right*" :selected t :width 49 :height 15 :margins (nil)) (:buffer "*golden-lower*" :selected nil :width 49 :height 9 :margins (nil)) (:buffer "*golden-main*" :selected nil :width 30 :height 24 :margins (nil))) ((:buffer "*golden-lower*" :selected t :width 49 :height 15 :margins (nil)) (:buffer "*golden-main*" :selected nil :width 30 :height 24 :margins (nil)) (:buffer "*golden-right*" :selected nil :width 49 :height 9 :margins (nil))) ((:buffer "*golden-main*" :selected t :width 49 :height 24 :margins (nil)) (:buffer "*golden-right*" :selected nil :width 30 :height 12 :margins (nil)) (:buffer "*golden-lower*" :selected nil :width 30 :height 12 :margins (nil))))"#
    ]];
    ParityBatchCase::value(
        "selecting_each_pane_rebalances_and_enlarges_the_active_editor",
        elisp_form,
        expected,
    )
}

fn mode_buffer_regexp_and_predicate_exclusions_leave_layout_untouched() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-golden-ratio-test-layout
 (lambda (buffers)
   (let ((golden-ratio-mode t)
         (golden-ratio-exclude-modes '(special-mode "text-mode"))
         (golden-ratio-exclude-buffer-names '("*golden-right*"))
         (golden-ratio-exclude-buffer-regexp '("lower\\*"))
         (golden-ratio-inhibit-functions
          (list (lambda () (eq major-mode 'emacs-lisp-mode))))
         results)
     (with-current-buffer (nth 0 buffers) (emacs-lisp-mode))
     (with-current-buffer (nth 1 buffers) (text-mode))
     (with-current-buffer (nth 2 buffers) (special-mode))
     (dolist (window (window-list nil 'nomini))
       (balance-windows)
       (select-window window)
       (let ((before (neomacs-golden-ratio-test-windows)))
         (golden-ratio)
         (push (list :buffer (buffer-name) :excluded
                     (equal before (neomacs-golden-ratio-test-windows))) results)))
     (setq golden-ratio-exclude-modes nil
           golden-ratio-exclude-buffer-names nil
           golden-ratio-exclude-buffer-regexp nil
           golden-ratio-inhibit-functions nil)
     (with-current-buffer (nth 0 buffers) (fundamental-mode))
     (select-window (get-buffer-window (nth 0 buffers)))
     (balance-windows)
     (let ((before (neomacs-golden-ratio-test-windows)))
       (golden-ratio)
       (list :excluded (nreverse results)
             :eligible-changed
             (not (equal before (neomacs-golden-ratio-test-windows)))
             :eligible-layout (neomacs-golden-ratio-test-windows))))))
"####;
    let expected = expect![[
        r#"OK (:excluded ((:buffer "*golden-right*" :excluded t) (:buffer "*golden-lower*" :excluded t) (:buffer "*golden-main*" :excluded t)) :eligible-changed t :eligible-layout ((:buffer "*golden-main*" :selected t :width 49 :height 23 :margins (nil)) (:buffer "*golden-right*" :selected nil :width 30 :height 12 :margins (nil)) (:buffer "*golden-lower*" :selected nil :width 30 :height 11 :margins (nil))))"#
    ]];
    ParityBatchCase::value(
        "mode_buffer_regexp_and_predicate_exclusions_leave_layout_untouched",
        elisp_form,
        expected,
    )
}

fn navigation_advice_resizes_the_newly_selected_window_and_restores_on_disable() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-golden-ratio-test-layout
 (lambda (_buffers)
   (unwind-protect
       (progn
         (golden-ratio-mode 1)
         (balance-windows)
         (let ((before (neomacs-golden-ratio-test-windows)))
           (other-window 1)
           (let ((after-navigation (neomacs-golden-ratio-test-windows))
                 (enabled-hooks
                  (list (memq 'golden-ratio window-configuration-change-hook)
                        (memq 'golden-ratio--post-command-hook post-command-hook)
                        (memq 'golden-ratio--mouse-leave-buffer-hook
                              mouse-leave-buffer-hook))))
             (golden-ratio-mode -1)
             (balance-windows)
             (let ((disabled-before (neomacs-golden-ratio-test-sizes)))
               (other-window 1)
               (list :before before :after-navigation after-navigation
                     :enabled-hooks (mapcar (lambda (x) (not (null x))) enabled-hooks)
                     :disabled-hooks
                     (list (memq 'golden-ratio window-configuration-change-hook)
                           (memq 'golden-ratio--post-command-hook post-command-hook)
                           (memq 'golden-ratio--mouse-leave-buffer-hook mouse-leave-buffer-hook))
                     :disabled-layout-unchanged
                     (equal disabled-before (neomacs-golden-ratio-test-sizes)))))))
     (golden-ratio-mode -1))))
"####;
    let expected = expect![[
        r#"OK (:before ((:buffer "*golden-right*" :selected t :width 40 :height 12 :margins (nil)) (:buffer "*golden-lower*" :selected nil :width 40 :height 11 :margins (nil)) (:buffer "*golden-main*" :selected nil :width 39 :height 23 :margins (nil))) :after-navigation ((:buffer "*golden-lower*" :selected t :width 49 :height 15 :margins (nil)) (:buffer "*golden-main*" :selected nil :width 30 :height 23 :margins (nil)) (:buffer "*golden-right*" :selected nil :width 49 :height 8 :margins (nil))) :enabled-hooks (t t t) :disabled-hooks (nil nil nil) :disabled-layout-unchanged nil)"#
    ]];
    ParityBatchCase::value(
        "navigation_advice_resizes_the_newly_selected_window_and_restores_on_disable",
        elisp_form,
        expected,
    )
}

fn extra_navigation_commands_and_mouse_transitions_schedule_deferred_resize() -> ParityBatchCase {
    let elisp_form = r####"
(let (scheduled)
  (cl-letf (((symbol-function 'run-with-idle-timer)
             (lambda (seconds repeat function &rest arguments)
               (push (list :idle seconds repeat (functionp function) arguments) scheduled)
               :idle-timer))
            ((symbol-function 'run-at-time)
             (lambda (seconds repeat function &rest arguments)
               (push (list :time seconds repeat (functionp function) arguments) scheduled)
               :timer)))
    (let ((golden-ratio-extra-commands '(windmove-right custom-jump)))
      (dolist (this-command
               '(self-insert-command windmove-right
                 (lambda () (interactive) (custom-jump))))
        (golden-ratio--post-command-hook))
      (golden-ratio--mouse-leave-buffer-hook)
      (nreverse scheduled))))
"####;
    let expected =
        expect!["OK ((:idle 0.01 nil t nil) (:idle 0.01 nil t nil) (:time 0.1 nil t nil))"];
    ParityBatchCase::value(
        "extra_navigation_commands_and_mouse_transitions_schedule_deferred_resize",
        elisp_form,
        expected,
    )
}

fn interactive_adjustments_apply_immediately_and_toggle_widescreen_state() -> ParityBatchCase {
    let elisp_form = r####"
(let ((golden-ratio-adjust-factor 1.0)
      (golden-ratio-wide-adjust-factor 0.72)
      calls)
  (cl-letf (((symbol-function 'golden-ratio)
             (lambda (&optional argument)
               (push (list golden-ratio-adjust-factor argument) calls))))
    (golden-ratio-toggle-widescreen)
    (golden-ratio-toggle-widescreen)
    (golden-ratio-adjust 0.55)
    (list :factor golden-ratio-adjust-factor :calls (nreverse calls))))
"####;
    let expected = expect!["OK (:factor 0.55 :calls ((0.72 nil) (1 nil) (0.55 nil)))"];
    ParityBatchCase::value(
        "interactive_adjustments_apply_immediately_and_toggle_widescreen_state",
        elisp_form,
        expected,
    )
}

#[test]
fn golden_ratio_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GOLDEN_RATIO_MELPA_PIN, "golden-ratio.el")
            .expect("prepare revision-pinned Golden Ratio source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "golden-ratio-package-batch",
        "Golden Ratio",
        &[
            scaling_controls_compute_editing_dimensions_for_real_frame_shapes(),
            selecting_each_pane_rebalances_and_enlarges_the_active_editor(),
            mode_buffer_regexp_and_predicate_exclusions_leave_layout_untouched(),
            navigation_advice_resizes_the_newly_selected_window_and_restores_on_disable(),
            extra_navigation_commands_and_mouse_transitions_schedule_deferred_resize(),
            interactive_adjustments_apply_immediately_and_toggle_widescreen_state(),
        ],
    );
}
