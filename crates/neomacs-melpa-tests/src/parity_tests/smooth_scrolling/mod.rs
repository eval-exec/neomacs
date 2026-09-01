//! Practical parity coverage for rank 418 `smooth-scrolling`.
//!
//! These cases drive the public global mode and its installed motion advice in
//! real windows while preserving the ambient scrolling and advice lifecycle.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SMOOTH_SCROLLING_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'smooth-scrolling)

(defconst ss418-test-upstream-main-sha
  "68ce888019df1a8faf5bccc40c87adc06b3de075c73870da8abf5ee0b46e34ff")
(defconst ss418-test-installed-main-sha
  "9d268ba1e212e11a97f4a509bd89f404f72d38558e67d4e517b3a94cb55f9434")
(defconst ss418-test-installed-pkg-sha
  "ada95567d3158c61f7e3d31c8e584ad48253d59cc110fcb26775b889d8927e4c")

(defun ss418-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun ss418-test-package-advice-state ()
  (mapcar
   (lambda (function)
     (list function
           (and (ad-find-advice function 'after 'smooth-scroll) t)))
   '(previous-line next-line dired-previous-line dired-next-line
     isearch-repeat scroll-up-command scroll-down-command)))

(defun ss418-test-source-state ()
  (let* ((located (locate-library "smooth-scrolling.el"))
         (main (file-truename located))
         (pkg (expand-file-name "smooth-scrolling-pkg.el"
                                (file-name-directory main)))
         (manifest
          (list (cons "smooth-scrolling-pkg.el" (ss418-test-file-sha pkg))
                (cons "smooth-scrolling.el" (ss418-test-file-sha main))))
         (advice (ss418-test-package-advice-state)))
    (unless (and (file-regular-p located) (not (file-symlink-p located))
                 (file-regular-p pkg) (not (file-symlink-p pkg))
                 (equal manifest
                        `(("smooth-scrolling-pkg.el"
                           . ,ss418-test-installed-pkg-sha)
                          ("smooth-scrolling.el"
                           . ,ss418-test-installed-main-sha)))
                 (equal advice
                        '((previous-line t) (next-line t)
                          (dired-previous-line t) (dired-next-line t)
                          (isearch-repeat t) (scroll-up-command t)
                          (scroll-down-command t))))
      (error "Smooth Scrolling installed source/advice mismatch: %S %S"
             manifest advice))
    (list :upstream-sha256 ss418-test-upstream-main-sha
          :installed-sha256 manifest
          :version
          (package-version-join
           (package-desc-version
            (cadr (assq 'smooth-scrolling package-alist))))
          :feature (featurep 'smooth-scrolling)
          :advice advice)))

(defun ss418-test-window-state ()
  (redisplay t)
  (list :point-line (line-number-at-pos (window-point (selected-window)))
        :start-line (line-number-at-pos (window-start (selected-window)))
        :height (window-body-height (selected-window))
        :above (smooth-scroll-lines-above-point)
        :below (smooth-scroll-lines-below-point)
        :allowed-margin (smooth-scroll-window-allowed-margin)))

(defun ss418-test-reset-view (start-line point-line)
  (goto-char (point-min))
  (forward-line (1- start-line))
  (set-window-start (selected-window) (point) 'noforce)
  (goto-char (point-min))
  (forward-line (1- point-line))
  (set-window-point (selected-window) (point))
  (redisplay t))

(defun ss418-test-with-window (name line-count body)
  (let ((buffer (generate-new-buffer (format "ss418-%s" name))))
    (unwind-protect
        (save-window-excursion
          (delete-other-windows)
          (switch-to-buffer buffer)
          (dotimes (index line-count)
            (insert (format "line-%03d payload 界\n" (1+ index))))
          (set-buffer-modified-p nil)
          (let ((test-window (split-window-below -16)))
            (set-window-buffer test-window buffer)
            (select-window test-window))
          (ss418-test-reset-view 1 1)
          (funcall body buffer))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun ss418-test-conditional-move (smoothp)
  (ignore smoothp)
  (forward-line 1))

(defun ss418-test-forbid-external (operation &rest arguments)
  (error "Unexpected Smooth Scrolling external boundary: %S %S"
         operation arguments))

(defun ss418-test-window-count ()
  (length (window-list nil 'no-minibuffer)))

(defun ss418-test-run (body)
  (let* ((buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (window-before (current-window-configuration))
         (window-count-before (ss418-test-window-count))
         (selected-window-before (selected-window))
         (window-buffer-before (window-buffer selected-window-before))
         (window-start-before (window-start selected-window-before))
         (window-point-before (window-point selected-window-before))
         (source-before (ss418-test-source-state))
         (mode-before smooth-scrolling-mode)
         (orig-margin-before smooth-scroll-orig-scroll-margin)
         (scroll-margin-before scroll-margin)
         (margin-before smooth-scroll-margin)
         (strict-before smooth-scroll-strict-margins)
         (smooth-scrolling-mode nil)
         (smooth-scroll-orig-scroll-margin nil)
         (scroll-margin 0)
         (smooth-scroll-margin 10)
         (smooth-scroll-strict-margins t)
         (scroll-conservatively 101)
         (scroll-step 1)
         (temporary-goal-column nil)
         (goal-column nil)
         (track-eol nil)
         result source-after cleanup-errors)
    (unwind-protect
        (cl-letf (((symbol-function 'call-process)
                   (lambda (&rest args)
                     (apply #'ss418-test-forbid-external 'call-process args)))
                  ((symbol-function 'call-process-region)
                   (lambda (&rest args)
                     (apply #'ss418-test-forbid-external
                            'call-process-region args)))
                  ((symbol-function 'make-process)
                   (lambda (&rest args)
                     (apply #'ss418-test-forbid-external 'make-process args)))
                  ((symbol-function 'process-file)
                   (lambda (&rest args)
                     (apply #'ss418-test-forbid-external 'process-file args)))
                  ((symbol-function 'start-file-process)
                   (lambda (&rest args)
                     (apply #'ss418-test-forbid-external
                            'start-file-process args)))
                  ((symbol-function 'start-process)
                   (lambda (&rest args)
                     (apply #'ss418-test-forbid-external 'start-process args)))
                  ((symbol-function 'url-retrieve)
                   (lambda (&rest args)
                     (apply #'ss418-test-forbid-external 'url-retrieve args)))
                  ((symbol-function 'url-retrieve-synchronously)
                   (lambda (&rest args)
                     (apply #'ss418-test-forbid-external
                            'url-retrieve-synchronously args))))
          (setq result (funcall body)
                source-after (ss418-test-source-state))
          (unless (equal source-before source-after)
            (error "Smooth Scrolling source/advice changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (attempt 'fixture-advice
                 (lambda ()
                   (disable-smooth-scroll-for-function
                    ss418-test-conditional-move)))
        (when smooth-scrolling-mode
          (attempt 'mode (lambda () (smooth-scrolling-mode -1))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda () (kill-buffer buffer)))))
        (dolist (timer (copy-sequence timer-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (setq smooth-scrolling-mode mode-before
              smooth-scroll-orig-scroll-margin orig-margin-before
              scroll-margin scroll-margin-before
              smooth-scroll-margin margin-before
              smooth-scroll-strict-margins strict-before)
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :fixture-advice-removed
                 (not (ad-find-advice
                       'ss418-test-conditional-move 'after 'smooth-scroll))
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer)
                                       (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process)
                                       (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :window-restored
                 (and (= window-count-before (ss418-test-window-count))
                      (eq (selected-window) selected-window-before)
                      (eq (window-buffer selected-window-before)
                          window-buffer-before)
                      (= (window-start selected-window-before)
                         window-start-before)
                      (= (window-point selected-window-before)
                         window-point-before))
                 :state-restored
                 (and (eq smooth-scrolling-mode mode-before)
                      (eq smooth-scroll-orig-scroll-margin orig-margin-before)
                      (eq scroll-margin scroll-margin-before)
                      (eq smooth-scroll-margin margin-before)
                      (eq smooth-scroll-strict-margins strict-before))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "Smooth Scrolling cleanup failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SMOOTH_SCROLLING_MELPA_PIN, "smooth-scrolling.el")
        .expect("prepare exact shallow Smooth Scrolling source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_global_mode_owns_and_restores_the_native_scroll_margin() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_global_mode_owns_and_restores_the_native_scroll_margin",
        r####"
(ss418-test-run
 (lambda ()
   (setq scroll-margin 7)
   (let ((before
          (list :mode smooth-scrolling-mode :scroll-margin scroll-margin
                :saved-margin smooth-scroll-orig-scroll-margin)))
     (smooth-scrolling-mode 1)
     (let ((enabled
            (list :mode smooth-scrolling-mode :scroll-margin scroll-margin
                  :saved-margin smooth-scroll-orig-scroll-margin)))
       (smooth-scrolling-mode -1)
       (list :before before :enabled enabled
             :disabled
             (list :mode smooth-scrolling-mode :scroll-margin scroll-margin
                   :saved-margin smooth-scroll-orig-scroll-margin))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "68ce888019df1a8faf5bccc40c87adc06b3de075c73870da8abf5ee0b46e34ff" :installed-sha256 (("smooth-scrolling-pkg.el" . "ada95567d3158c61f7e3d31c8e584ad48253d59cc110fcb26775b889d8927e4c") ("smooth-scrolling.el" . "9d268ba1e212e11a97f4a509bd89f404f72d38558e67d4e517b3a94cb55f9434")) :version "20161002.1949" :feature t :advice ((previous-line t) (next-line t) (dired-previous-line t) (dired-next-line t) (isearch-repeat t) (scroll-up-command t) (scroll-down-command t))) :result (:before (:mode nil :scroll-margin 7 :saved-margin nil) :enabled (:mode t :scroll-margin 0 :saved-margin 7) :disabled (:mode nil :scroll-margin 7 :saved-margin nil)) :cleanup (:source-unchanged t :fixture-advice-removed t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn advised_line_motion_preserves_context_above_and_below_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "advised_line_motion_preserves_context_above_and_below_point",
        r####"
(ss418-test-run
 (lambda ()
   (ss418-test-with-window
    "line-motion" 100
    (lambda (_buffer)
      (setq smooth-scroll-margin 4
            smooth-scroll-strict-margins nil)
      (ss418-test-reset-view 1 13)
      (let ((disabled-before (ss418-test-window-state)))
        (next-line 1)
        (let ((disabled-after (ss418-test-window-state)))
          (smooth-scrolling-mode 1)
          (ss418-test-reset-view 1 13)
          (let ((down-before (ss418-test-window-state)))
            (next-line 1)
            (let ((down-after (ss418-test-window-state)))
              (ss418-test-reset-view 20 22)
              (let ((up-before (ss418-test-window-state)))
                (previous-line 1)
                (let ((up-after (ss418-test-window-state)))
                  (smooth-scrolling-mode -1)
                  (list :disabled (list disabled-before disabled-after)
                        :enabled-down (list down-before down-after)
                        :enabled-up (list up-before up-after))))))))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "68ce888019df1a8faf5bccc40c87adc06b3de075c73870da8abf5ee0b46e34ff" :installed-sha256 (("smooth-scrolling-pkg.el" . "ada95567d3158c61f7e3d31c8e584ad48253d59cc110fcb26775b889d8927e4c") ("smooth-scrolling.el" . "9d268ba1e212e11a97f4a509bd89f404f72d38558e67d4e517b3a94cb55f9434")) :version "20161002.1949" :feature t :advice ((previous-line t) (next-line t) (dired-previous-line t) (dired-next-line t) (isearch-repeat t) (scroll-up-command t) (scroll-down-command t))) :result (:disabled ((:point-line 13 :start-line 1 :height 15 :above 12 :below 2 :allowed-margin 7) (:point-line 14 :start-line 1 :height 15 :above 13 :below 1 :allowed-margin 7)) :enabled-down ((:point-line 13 :start-line 1 :height 15 :above 12 :below 2 :allowed-margin 7) (:point-line 14 :start-line 8 :height 15 :above 6 :below 8 :allowed-margin 7)) :enabled-up ((:point-line 22 :start-line 20 :height 15 :above 2 :below 12 :allowed-margin 7) (:point-line 21 :start-line 13 :height 15 :above 8 :below 6 :allowed-margin 7))) :cleanup (:source-unchanged t :fixture-advice-removed t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn strict_margins_count_wrapped_visual_lines_before_scrolling() -> ParityBatchCase {
    ParityBatchCase::value(
        "strict_margins_count_wrapped_visual_lines_before_scrolling",
        r####"
(ss418-test-run
 (lambda ()
   (ss418-test-with-window
    "wrapped-lines" 30
    (lambda (_buffer)
      (save-excursion
        (goto-char (point-min))
        (forward-line 8)
        (end-of-line)
        (insert (make-string 180 ?x) " 界"))
      (setq smooth-scroll-margin 4)
      (smooth-scrolling-mode 1)
      (let (logical visual)
        (setq smooth-scroll-strict-margins nil)
        (ss418-test-reset-view 1 9)
        (let ((before (ss418-test-window-state)))
          (next-line 1)
          (setq logical (list before (ss418-test-window-state))))
        (setq smooth-scroll-strict-margins t)
        (ss418-test-reset-view 1 9)
        (let ((before (ss418-test-window-state)))
          (next-line 1)
          (setq visual (list before (ss418-test-window-state))))
        (smooth-scrolling-mode -1)
        (list :logical logical :visual visual))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "68ce888019df1a8faf5bccc40c87adc06b3de075c73870da8abf5ee0b46e34ff" :installed-sha256 (("smooth-scrolling-pkg.el" . "ada95567d3158c61f7e3d31c8e584ad48253d59cc110fcb26775b889d8927e4c") ("smooth-scrolling.el" . "9d268ba1e212e11a97f4a509bd89f404f72d38558e67d4e517b3a94cb55f9434")) :version "20161002.1949" :feature t :advice ((previous-line t) (next-line t) (dired-previous-line t) (dired-next-line t) (isearch-repeat t) (scroll-up-command t) (scroll-down-command t))) :result (:logical ((:point-line 9 :start-line 1 :height 15 :above 8 :below 6 :allowed-margin 7) (:point-line 10 :start-line 1 :height 15 :above 9 :below 5 :allowed-margin 7)) :visual ((:point-line 9 :start-line 1 :height 15 :above 8 :below 6 :allowed-margin 7) (:point-line 10 :start-line 6 :height 15 :above 6 :below 8 :allowed-margin 7))) :cleanup (:source-unchanged t :fixture-advice-removed t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn short_windows_clamp_large_requested_margins_without_overlap() -> ParityBatchCase {
    ParityBatchCase::value(
        "short_windows_clamp_large_requested_margins_without_overlap",
        r####"
(ss418-test-run
 (lambda ()
   (ss418-test-with-window
    "short-window" 60
    (lambda (buffer)
      (let ((lower (split-window-below -9)))
        (set-window-buffer lower buffer)
        (select-window lower)
        (setq smooth-scroll-margin 10
              smooth-scroll-strict-margins nil)
        (smooth-scrolling-mode 1)
        (ss418-test-reset-view 21 25)
        (let ((before (ss418-test-window-state)))
          (next-line 1)
          (let ((after (ss418-test-window-state)))
            (smooth-scrolling-mode -1)
            (list :requested smooth-scroll-margin
                  :before before :after after))))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "68ce888019df1a8faf5bccc40c87adc06b3de075c73870da8abf5ee0b46e34ff" :installed-sha256 (("smooth-scrolling-pkg.el" . "ada95567d3158c61f7e3d31c8e584ad48253d59cc110fcb26775b889d8927e4c") ("smooth-scrolling.el" . "9d268ba1e212e11a97f4a509bd89f404f72d38558e67d4e517b3a94cb55f9434")) :version "20161002.1949" :feature t :advice ((previous-line t) (next-line t) (dired-previous-line t) (dired-next-line t) (isearch-repeat t) (scroll-up-command t) (scroll-down-command t))) :result (:requested 10 :before (:point-line 25 :start-line 21 :height 8 :above 4 :below 3 :allowed-margin 3) :after (:point-line 26 :start-line 23 :height 8 :above 3 :below 4 :allowed-margin 3)) :cleanup (:source-unchanged t :fixture-advice-removed t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn conditional_extension_advice_routes_and_then_detaches_cleanly() -> ParityBatchCase {
    ParityBatchCase::value(
        "conditional_extension_advice_routes_and_then_detaches_cleanly",
        r####"
(ss418-test-run
 (lambda ()
   (ss418-test-with-window
    "conditional-extension" 100
    (lambda (_buffer)
      (setq smooth-scroll-margin 4
            smooth-scroll-strict-margins nil)
      (smooth-scrolling-mode 1)
      (enable-smooth-scroll-for-function-conditionally
          ss418-test-conditional-move smoothp)
      (let ((installed
             (and (ad-find-advice
                   'ss418-test-conditional-move 'after 'smooth-scroll) t))
            plain smoothed detached)
        (ss418-test-reset-view 1 13)
        (ss418-test-conditional-move nil)
        (setq plain (ss418-test-window-state))
        (ss418-test-reset-view 1 13)
        (ss418-test-conditional-move t)
        (setq smoothed (ss418-test-window-state))
        (disable-smooth-scroll-for-function ss418-test-conditional-move)
        (ss418-test-reset-view 1 13)
        (ss418-test-conditional-move t)
        (setq detached (ss418-test-window-state))
        (smooth-scrolling-mode -1)
        (list :installed installed :plain plain :smoothed smoothed
              :detached detached
              :removed
              (not (ad-find-advice
                    'ss418-test-conditional-move 'after 'smooth-scroll))))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "68ce888019df1a8faf5bccc40c87adc06b3de075c73870da8abf5ee0b46e34ff" :installed-sha256 (("smooth-scrolling-pkg.el" . "ada95567d3158c61f7e3d31c8e584ad48253d59cc110fcb26775b889d8927e4c") ("smooth-scrolling.el" . "9d268ba1e212e11a97f4a509bd89f404f72d38558e67d4e517b3a94cb55f9434")) :version "20161002.1949" :feature t :advice ((previous-line t) (next-line t) (dired-previous-line t) (dired-next-line t) (isearch-repeat t) (scroll-up-command t) (scroll-down-command t))) :result (:installed t :plain (:point-line 14 :start-line 1 :height 15 :above 13 :below 1 :allowed-margin 7) :smoothed (:point-line 14 :start-line 8 :height 15 :above 6 :below 8 :allowed-margin 7) :detached (:point-line 14 :start-line 1 :height 15 :above 13 :below 1 :allowed-margin 7) :removed t) :cleanup (:source-unchanged t :fixture-advice-removed t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :window-restored t :state-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn smooth_scrolling_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_global_mode_owns_and_restores_the_native_scroll_margin(),
        advised_line_motion_preserves_context_above_and_below_point(),
        strict_margins_count_wrapped_visual_lines_before_scrolling(),
        short_windows_clamp_large_requested_margins_without_overlap(),
        conditional_extension_advice_routes_and_then_detaches_cleanly(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "smooth-scrolling-rank418",
        "smooth_scrolling_parity",
        &cases,
    );
}
