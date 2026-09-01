use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, POPWIN_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const POPWIN_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const POPWIN_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'popwin)

(defun neomacs-popwin-test-layout ()
  "Describe live non-minibuffer windows in visual order."
  (let* ((windows (window-list nil 'nomini))
         (frame-top (apply #'min
                           (mapcar (lambda (window)
                                     (nth 1 (window-edges window)))
                                   windows)))
         (frame-bottom (apply #'max
                              (mapcar (lambda (window)
                                        (nth 3 (window-edges window)))
                                      windows))))
    (mapcar
     (lambda (window)
       (let* ((edges (window-edges window))
              (top (nth 1 edges))
              (bottom (nth 3 edges))
              (popup (eq window popwin:popup-window))
              (vertical
               (cond
                ((and (= top frame-top) (= bottom frame-bottom)) 'full)
                ((= top frame-top) 'top)
                ((= bottom frame-bottom) 'bottom)
                (t 'middle))))
         (list :buffer (buffer-name (window-buffer window))
               :x-range (list (nth 0 edges) (nth 2 edges))
               :vertical vertical
               :popup-size
               (and popup
                    (if (eq vertical 'full)
                        (list :width (- (nth 2 edges) (nth 0 edges)))
                      (list :height (- bottom top))))
               :selected (eq window (selected-window))
               :dedicated (window-dedicated-p window)
               :point (window-point window))))
     (sort windows
           (lambda (left right)
             (let ((left-edges (window-edges left))
                   (right-edges (window-edges right)))
               (or (< (nth 1 left-edges) (nth 1 right-edges))
                   (and (= (nth 1 left-edges) (nth 1 right-edges))
                        (< (nth 0 left-edges) (nth 0 right-edges))))))))))

(defun neomacs-popwin-test-buffer (name &optional contents mode)
  "Create a clean test buffer NAME containing CONTENTS in MODE."
  (let ((buffer (get-buffer-create name)))
    (with-current-buffer buffer
      (erase-buffer)
      (insert (or contents ""))
      (goto-char (point-min))
      (when mode (funcall mode)))
    buffer))

(defun neomacs-popwin-test-reset ()
  "Cancel Popwin infrastructure and discard all test buffers."
  (ignore-errors (popwin:close-popup-window))
  (popwin:stop-close-popup-window-timer)
  (popwin:kill-dummy-buffer)
  (setq popwin:context-stack nil
        popwin:popup-window nil
        popwin:popup-buffer nil
        popwin:popup-last-config nil
        popwin:master-window nil
        popwin:focus-window nil
        popwin:selected-window nil
        popwin:popup-window-dedicated-p nil
        popwin:popup-window-stuck-p nil
        popwin:window-outline nil
        popwin:window-map nil
        popwin:window-config nil)
  (dolist (buffer (buffer-list))
    (when (string-prefix-p " *neomacs-popwin-" (buffer-name buffer))
      (kill-buffer buffer))))

(defun neomacs-popwin-test-run (function)
  "Run FUNCTION in a clean frame-shaped window sandbox."
  (save-window-excursion
    (neomacs-popwin-test-reset)
    (delete-other-windows)
    (unwind-protect
        (funcall function)
      (neomacs-popwin-test-reset))))
"###;

fn popwin_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(POPWIN_MELPA_PIN, "popwin.el")
        .expect("prepare revision-pinned Popwin source below ./tmp")
        .with_prelude(POPWIN_TEST_PRELUDE)
        .with_timeout(POPWIN_TEST_TIMEOUT)
}

fn build_log_popup_restores_a_three_pane_workspace_points_and_selection() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-popwin-test-run
 (lambda ()
   (let* ((source (neomacs-popwin-test-buffer
                   " *neomacs-popwin-source*"
                   "(deploy-release 'REL-2048)\n"))
          (tests (neomacs-popwin-test-buffer
                  " *neomacs-popwin-tests*"
                  "test_release_canary\ntest_release_stable\n"))
          (notes (neomacs-popwin-test-buffer
                  " *neomacs-popwin-notes*"
                  "* REL-2048\n- verify canary\n"))
          (build (neomacs-popwin-test-buffer
                  " *neomacs-popwin-build*"
                  "Compiling release\nFinished successfully\n"))
          (left (selected-window))
          (middle (split-window-right))
          (right (split-window-right nil middle)))
     (set-window-buffer left source)
     (set-window-buffer middle tests)
     (set-window-buffer right notes)
     (set-window-point left 10)
     (set-window-point middle 8)
     (set-window-point right 5)
     (select-window left)
     (let ((before (neomacs-popwin-test-layout)))
       (popwin:popup-buffer build :position 'bottom :height 6
                            :noselect t :dedicated t)
       (let ((during (neomacs-popwin-test-layout))
             (popup-state
              (list :live (popwin:popup-window-live-p)
                    :buffer (buffer-name popwin:popup-buffer)
                    :selected (buffer-name (window-buffer (selected-window)))
                    :dedicated-request popwin:popup-window-dedicated-p
                    :actual-dedication
                    (window-dedicated-p popwin:popup-window))))
         (popwin:close-popup-window)
         (list :before before
               :during during
               :popup popup-state
               :after (neomacs-popwin-test-layout)
               :context-depth (length popwin:context-stack)))))))
"###;
    let expected = expect![[
        r####"OK (:before ((:buffer " *neomacs-popwin-source*" :x-range (0 40) :vertical full :popup-size nil :selected t :dedicated nil :point 10) (:buffer " *neomacs-popwin-tests*" :x-range (40 60) :vertical full :popup-size nil :selected nil :dedicated nil :point 8) (:buffer " *neomacs-popwin-notes*" :x-range (60 80) :vertical full :popup-size nil :selected nil :dedicated nil :point 5)) :during ((:buffer " *neomacs-popwin-source*" :x-range (0 40) :vertical top :popup-size nil :selected t :dedicated nil :point 10) (:buffer " *neomacs-popwin-tests*" :x-range (40 60) :vertical top :popup-size nil :selected nil :dedicated nil :point 8) (:buffer " *neomacs-popwin-notes*" :x-range (60 80) :vertical top :popup-size nil :selected nil :dedicated nil :point 5) (:buffer " *neomacs-popwin-build*" :x-range (0 80) :vertical bottom :popup-size (:height 6) :selected nil :dedicated nil :point 1)) :popup (:live t :buffer " *neomacs-popwin-build*" :selected " *neomacs-popwin-source*" :dedicated-request t :actual-dedication nil) :after ((:buffer " *neomacs-popwin-source*" :x-range (0 40) :vertical full :popup-size nil :selected t :dedicated nil :point 10) (:buffer " *neomacs-popwin-tests*" :x-range (40 60) :vertical full :popup-size nil :selected nil :dedicated nil :point 8) (:buffer " *neomacs-popwin-notes*" :x-range (60 80) :vertical full :popup-size nil :selected nil :dedicated nil :point 5)) :context-depth 0)"####
    ]];
    ParityBatchCase::value(
        "build_log_popup_restores_a_three_pane_workspace_points_and_selection",
        elisp_form,
        expected,
    )
}

fn directional_popup_configuration_places_logs_on_every_requested_edge() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-popwin-test-run
 (lambda ()
   (let ((editor (neomacs-popwin-test-buffer
                  " *neomacs-popwin-editor*" "release plan\n"))
         (popup (neomacs-popwin-test-buffer
                 " *neomacs-popwin-directional*" "status: healthy\n"))
         reports)
     (dolist (config '((left :width 20)
                       (top :height 6)
                       (right :width 0.25)
                       (bottom :height 0.25)))
       (save-window-excursion
         (delete-other-windows)
         (switch-to-buffer editor)
         (let ((position (car config))
               (size-key (cadr config))
               (size (caddr config)))
           (apply #'popwin:popup-buffer popup
                  :position position :noselect t
                  (list size-key size))
           (let ((edges (window-edges popwin:popup-window)))
             (push
              (list :position position
                    :layout (neomacs-popwin-test-layout)
                    :popup-size
                    (if (memq position '(left right))
                        (list :width (- (nth 2 edges) (nth 0 edges)))
                      (list :height (- (nth 3 edges) (nth 1 edges)))))
              reports))
           (popwin:close-popup-window))))
     (nreverse reports))))
"###;
    let expected = expect![[
        r####"OK ((:position left :layout ((:buffer " *neomacs-popwin-directional*" :x-range (0 20) :vertical full :popup-size (:width 20) :selected nil :dedicated nil :point 1) (:buffer " *neomacs-popwin-editor*" :x-range (20 80) :vertical full :popup-size nil :selected t :dedicated nil :point 1)) :popup-size (:width 20)) (:position top :layout ((:buffer " *neomacs-popwin-directional*" :x-range (0 80) :vertical top :popup-size (:height 6) :selected nil :dedicated nil :point 1) (:buffer " *neomacs-popwin-editor*" :x-range (0 80) :vertical bottom :popup-size nil :selected t :dedicated nil :point 1)) :popup-size (:height 6)) (:position right :layout ((:buffer " *neomacs-popwin-editor*" :x-range (0 60) :vertical full :popup-size nil :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-directional*" :x-range (60 80) :vertical full :popup-size (:width 20) :selected nil :dedicated nil :point 1)) :popup-size (:width 20)) (:position bottom :layout ((:buffer " *neomacs-popwin-editor*" :x-range (0 80) :vertical top :popup-size nil :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-directional*" :x-range (0 80) :vertical bottom :popup-size (:height 6) :selected nil :dedicated nil :point 1)) :popup-size (:height 6)))"####
    ]];
    ParityBatchCase::value(
        "directional_popup_configuration_places_logs_on_every_requested_edge",
        elisp_form,
        expected,
    )
}

fn display_buffer_routes_help_by_mode_and_leaves_normal_buffers_to_emacs() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-popwin-test-run
 (lambda ()
   (let* ((display-buffer-alist nil)
          (popwin:special-display-config
           '((help-mode :height 5 :position bottom
                        :noselect t :dedicated t)))
          (editor (neomacs-popwin-test-buffer
                   " *neomacs-popwin-routing-editor*" "release plan\n"))
          (help (neomacs-popwin-test-buffer
                 " *neomacs-popwin-routing-help*"
                 "Release command documentation\n" 'help-mode))
          (normal (neomacs-popwin-test-buffer
                   " *neomacs-popwin-routing-normal*" "ordinary notes\n")))
     (switch-to-buffer editor)
     (unwind-protect
         (progn
           (popwin-mode 1)
           (let* ((special-window (display-buffer help))
                  (special
                   (list :returned-buffer
                         (buffer-name (window-buffer special-window))
                         :popup (eq special-window popwin:popup-window)
                         :selected (buffer-name (window-buffer (selected-window)))
                         :layout (neomacs-popwin-test-layout)
                         :route (popwin:match-config help))))
             (popwin:close-popup-window)
             (let ((normal-window (display-buffer normal)))
               (list :special special
                     :normal
                     (list :returned-buffer
                           (buffer-name (window-buffer normal-window))
                           :popup-live (popwin:popup-window-live-p)
                           :layout (neomacs-popwin-test-layout))
                     :display-rule-installed
                     (and (member '(popwin:display-buffer-condition
                                    popwin:display-buffer-action)
                                  display-buffer-alist)
                          t)))))
       (popwin-mode -1)))))
"###;
    let expected = expect![[
        r####"OK (:special (:returned-buffer " *neomacs-popwin-routing-help*" :popup t :selected " *neomacs-popwin-routing-editor*" :layout ((:buffer " *neomacs-popwin-routing-editor*" :x-range (0 80) :vertical top :popup-size nil :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-routing-help*" :x-range (0 80) :vertical bottom :popup-size (:height 5) :selected nil :dedicated nil :point 1)) :route (help-mode :height 5 :position bottom :noselect t :dedicated t)) :normal (:returned-buffer " *neomacs-popwin-routing-normal*" :popup-live nil :layout ((:buffer " *neomacs-popwin-routing-editor*" :x-range (0 80) :vertical top :popup-size nil :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-routing-normal*" :x-range (0 80) :vertical bottom :popup-size nil :selected nil :dedicated nil :point 1))) :display-rule-installed t)"####
    ]];
    ParityBatchCase::value(
        "display_buffer_routes_help_by_mode_and_leaves_normal_buffers_to_emacs",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn nested_diagnostic_popups_unwind_to_the_previous_popup_then_editor() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-popwin-test-run
 (lambda ()
   (let ((editor (neomacs-popwin-test-buffer
                  " *neomacs-popwin-nested-editor*" "deploy release\n"))
         (build (neomacs-popwin-test-buffer
                 " *neomacs-popwin-nested-build*" "build complete\n"))
         (warnings (neomacs-popwin-test-buffer
                    " *neomacs-popwin-nested-warnings*" "warning: retry\n")))
     (switch-to-buffer editor)
     (popwin:popup-buffer build :position 'bottom :height 6)
     (let ((first (list :layout (neomacs-popwin-test-layout)
                        :popup (buffer-name popwin:popup-buffer)
                        :depth (length popwin:context-stack))))
       (popwin:popup-buffer warnings :position 'right :width 22)
       (let ((second (list :layout (neomacs-popwin-test-layout)
                           :popup (buffer-name popwin:popup-buffer)
                           :depth (length popwin:context-stack))))
         (popwin:close-popup-window)
         (let ((unwound (list :layout (neomacs-popwin-test-layout)
                              :popup (and popwin:popup-buffer
                                          (buffer-name popwin:popup-buffer))
                              :depth (length popwin:context-stack))))
           (popwin:close-popup-window)
           (list :first first :second second :unwound unwound
                 :restored (neomacs-popwin-test-layout))))))))
"###;
    let expected = expect![[
        r####"OK (:first (:layout ((:buffer " *neomacs-popwin-nested-editor*" :x-range (0 80) :vertical top :popup-size nil :selected nil :dedicated nil :point 1) (:buffer " *neomacs-popwin-nested-build*" :x-range (0 80) :vertical bottom :popup-size (:height 6) :selected t :dedicated nil :point 1)) :popup " *neomacs-popwin-nested-build*" :depth 1) :second (:layout ((:buffer " *neomacs-popwin-nested-editor*" :x-range (0 58) :vertical top :popup-size nil :selected nil :dedicated nil :point 1) (:buffer " *neomacs-popwin-nested-warnings*" :x-range (58 80) :vertical full :popup-size (:width 22) :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-nested-build*" :x-range (0 58) :vertical bottom :popup-size nil :selected nil :dedicated nil :point 1)) :popup " *neomacs-popwin-nested-warnings*" :depth 2) :unwound (:layout ((:buffer " *neomacs-popwin-nested-editor*" :x-range (0 80) :vertical top :popup-size nil :selected nil :dedicated nil :point 1) (:buffer " *neomacs-popwin-nested-build*" :x-range (0 80) :vertical bottom :popup-size (:height 6) :selected t :dedicated nil :point 1)) :popup " *neomacs-popwin-nested-build*" :depth 1) :restored ((:buffer " *neomacs-popwin-nested-editor*" :x-range (0 80) :vertical full :popup-size nil :selected t :dedicated nil :point 1)))"####
    ]];
    ParityBatchCase::value(
        "nested_diagnostic_popups_unwind_to_the_previous_popup_then_editor",
        elisp_form,
        expected,
    )
}

fn sticky_popup_survives_focus_change_while_normal_popup_closes() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-popwin-test-run
 (lambda ()
   (let* ((editor (neomacs-popwin-test-buffer
                   " *neomacs-popwin-stick-editor*" "release plan\n"))
          (notes (neomacs-popwin-test-buffer
                  " *neomacs-popwin-stick-notes*" "operator notes\n"))
          (status (neomacs-popwin-test-buffer
                   " *neomacs-popwin-stick-status*" "canary healthy\n"))
          (left (selected-window))
          (right (split-window-right)))
     (set-window-buffer left editor)
     (set-window-buffer right notes)
     (select-window left)
     (popwin:popup-buffer status :position 'bottom :height 6 :noselect t)
     (select-window (get-buffer-window notes))
     (popwin:close-popup-window-if-necessary)
     (let ((normal (list :popup-live (popwin:popup-window-live-p)
                         :layout (neomacs-popwin-test-layout))))
       (setq left (get-buffer-window editor))
       (select-window left)
       (popwin:popup-buffer status :position 'bottom :height 6
                            :noselect t :stick t)
       (select-window (get-buffer-window notes))
       (popwin:close-popup-window-if-necessary)
       (let ((sticky (list :popup-live (popwin:popup-window-live-p)
                           :stuck popwin:popup-window-stuck-p
                           :layout (neomacs-popwin-test-layout))))
         (popwin:close-popup-window)
         (list :normal normal :sticky sticky
               :restored (neomacs-popwin-test-layout)))))))
"###;
    let expected = expect![[
        r####"OK (:normal (:popup-live nil :layout ((:buffer " *neomacs-popwin-stick-editor*" :x-range (0 40) :vertical full :popup-size nil :selected nil :dedicated nil :point 1) (:buffer " *neomacs-popwin-stick-notes*" :x-range (40 80) :vertical full :popup-size nil :selected t :dedicated nil :point 1))) :sticky (:popup-live t :stuck t :layout ((:buffer " *neomacs-popwin-stick-editor*" :x-range (0 40) :vertical top :popup-size nil :selected nil :dedicated nil :point 1) (:buffer " *neomacs-popwin-stick-notes*" :x-range (40 80) :vertical top :popup-size nil :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-stick-status*" :x-range (0 80) :vertical bottom :popup-size (:height 6) :selected nil :dedicated nil :point 1))) :restored ((:buffer " *neomacs-popwin-stick-editor*" :x-range (0 40) :vertical full :popup-size nil :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-stick-notes*" :x-range (40 80) :vertical full :popup-size nil :selected nil :dedicated nil :point 1)))"####
    ]];
    ParityBatchCase::value(
        "sticky_popup_survives_focus_change_while_normal_popup_closes",
        elisp_form,
        expected,
    )
}

fn dedicated_popup_hands_an_accidentally_selected_buffer_back_to_the_editor() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-popwin-test-run
 (lambda ()
   (let ((editor (neomacs-popwin-test-buffer
                  " *neomacs-popwin-dedicated-editor*" "release plan\n"))
         (diagnostics (neomacs-popwin-test-buffer
                       " *neomacs-popwin-dedicated-diagnostics*"
                       "warning at deploy.el:42\n"))
         (source (neomacs-popwin-test-buffer
                  " *neomacs-popwin-dedicated-source*"
                  "line one\nline two\nline three\n")))
     (switch-to-buffer editor)
     (let ((editor-window (selected-window)))
       (popwin:popup-buffer diagnostics :position 'bottom :height 6 :dedicated t)
       (switch-to-buffer source)
       (goto-char 12)
       (popwin:close-popup-window-if-necessary)
       (list :popup-live (popwin:popup-window-live-p)
             :editor-window-buffer (buffer-name (window-buffer editor-window))
             :selected-buffer (buffer-name (window-buffer (selected-window)))
             :selected-point (point)
             :layout (neomacs-popwin-test-layout))))))
"###;
    let expected = expect![[
        r####"OK (:popup-live nil :editor-window-buffer " *neomacs-popwin-dedicated-source*" :selected-buffer " *neomacs-popwin-dedicated-source*" :selected-point 12 :layout ((:buffer " *neomacs-popwin-dedicated-source*" :x-range (0 80) :vertical full :popup-size nil :selected t :dedicated nil :point 12)))"####
    ]];
    ParityBatchCase::value(
        "dedicated_popup_hands_an_accidentally_selected_buffer_back_to_the_editor",
        elisp_form,
        expected,
    )
}

fn tail_reopen_and_visible_buffer_reuse_preserve_operational_context() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-popwin-test-run
 (lambda ()
   (let ((editor (neomacs-popwin-test-buffer
                  " *neomacs-popwin-tail-editor*" "release plan\n"))
         (log (neomacs-popwin-test-buffer
               " *neomacs-popwin-tail-log*"
               (mapconcat (lambda (n) (format "event-%02d\n" n))
                          (number-sequence 1 30) ""))))
     (switch-to-buffer editor)
     (popwin:popup-buffer-tail log :position 'bottom :height 6 :noselect t)
     (let ((tail (list :window-point (window-point popwin:popup-window)
                       :point-max (with-current-buffer log (point-max))
                       :selected (buffer-name (window-buffer (selected-window)))
                       :layout (neomacs-popwin-test-layout))))
       (popwin:close-popup-window)
       (popwin:popup-last-buffer t)
       (let ((reopened
              (list :popup (buffer-name popwin:popup-buffer)
                    :noselect (not (eq (selected-window) popwin:popup-window))
                    :window-point (window-point popwin:popup-window)
                    :layout (neomacs-popwin-test-layout))))
         (popwin:close-popup-window)
         (switch-to-buffer log)
         (let ((popwin:special-display-config
                '((" *neomacs-popwin-tail-log*" :height 6))))
           (popwin:display-buffer log)
           (list :tail tail :reopened reopened
                 :reuse (list :popup-live (popwin:popup-window-live-p)
                              :windows (length (window-list nil 'nomini))
                              :selected
                              (buffer-name (window-buffer (selected-window)))))))))))
"###;
    let expected = expect![[
        r####"OK (:tail (:window-point 271 :point-max 271 :selected " *neomacs-popwin-tail-editor*" :layout ((:buffer " *neomacs-popwin-tail-editor*" :x-range (0 80) :vertical top :popup-size nil :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-tail-log*" :x-range (0 80) :vertical bottom :popup-size (:height 6) :selected nil :dedicated nil :point 271))) :reopened (:popup " *neomacs-popwin-tail-log*" :noselect t :window-point 271 :layout ((:buffer " *neomacs-popwin-tail-editor*" :x-range (0 80) :vertical top :popup-size nil :selected t :dedicated nil :point 1) (:buffer " *neomacs-popwin-tail-log*" :x-range (0 80) :vertical bottom :popup-size (:height 6) :selected nil :dedicated nil :point 271))) :reuse (:popup-live nil :windows 1 :selected " *neomacs-popwin-tail-log*"))"####
    ]];
    ParityBatchCase::value(
        "tail_reopen_and_visible_buffer_reuse_preserve_operational_context",
        elisp_form,
        expected,
    )
}

fn killed_and_buried_popup_buffers_close_once_and_run_lifecycle_hooks() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-popwin-test-run
 (lambda ()
   (let ((editor (neomacs-popwin-test-buffer
                  " *neomacs-popwin-lifecycle-editor*" "release plan\n"))
         (killed (neomacs-popwin-test-buffer
                  " *neomacs-popwin-lifecycle-killed*" "fatal error\n"))
         (buried (neomacs-popwin-test-buffer
                  " *neomacs-popwin-lifecycle-buried*" "completed\n"))
         events)
     (switch-to-buffer editor)
     (let ((popwin:before-popup-hook
            (list (lambda ()
                    (push (list :before (length (window-list nil 'nomini))) events))))
           (popwin:after-popup-hook
            (list (lambda ()
                    (push (list :after
                                (buffer-name popwin:popup-buffer)
                                (length (window-list nil 'nomini)))
                          events)))))
       (popwin:popup-buffer killed :position 'bottom :height 6 :noselect t)
       (kill-buffer killed)
       (popwin:close-popup-window-if-necessary)
       (let ((after-kill (list :popup-live (popwin:popup-window-live-p)
                               :layout (neomacs-popwin-test-layout))))
         (popwin:popup-buffer buried :position 'bottom :height 6 :noselect t)
         (bury-buffer buried)
         (popwin:close-popup-window-if-necessary)
         (list :after-kill after-kill
               :after-bury
               (list :popup-live (popwin:popup-window-live-p)
                     :buffer-live (buffer-live-p buried)
                     :layout (neomacs-popwin-test-layout))
               :events (nreverse events)))))))
"###;
    let expected = expect![[
        r####"OK (:after-kill (:popup-live nil :layout ((:buffer " *neomacs-popwin-lifecycle-editor*" :x-range (0 80) :vertical full :popup-size nil :selected t :dedicated nil :point 1))) :after-bury (:popup-live nil :buffer-live t :layout ((:buffer " *neomacs-popwin-lifecycle-editor*" :x-range (0 80) :vertical full :popup-size nil :selected t :dedicated nil :point 1))) :events ((:before 1) (:after " *neomacs-popwin-lifecycle-killed*" 2) (:before 1) (:after " *neomacs-popwin-lifecycle-buried*" 2)))"####
    ]];
    ParityBatchCase::value(
        "killed_and_buried_popup_buffers_close_once_and_run_lifecycle_hooks",
        elisp_form,
        expected,
    )
}

#[test]
fn popwin_package_batch() {
    assert_oracle_batch_cases(
        popwin_oracle(),
        "popwin-package-batch",
        "popwin",
        &[
            build_log_popup_restores_a_three_pane_workspace_points_and_selection(),
            directional_popup_configuration_places_logs_on_every_requested_edge(),
            display_buffer_routes_help_by_mode_and_leaves_normal_buffers_to_emacs(),
            nested_diagnostic_popups_unwind_to_the_previous_popup_then_editor(),
            sticky_popup_survives_focus_change_while_normal_popup_closes(),
            dedicated_popup_hands_an_accidentally_selected_buffer_back_to_the_editor(),
            tail_reopen_and_visible_buffer_reuse_preserve_operational_context(),
            killed_and_buried_popup_buffers_close_once_and_run_lifecycle_hooks(),
        ],
    );
}
