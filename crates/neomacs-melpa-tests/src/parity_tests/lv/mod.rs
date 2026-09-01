use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LV_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const LV_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const LV_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'lv)

(defvar lv-test-hook-events nil)

(defun lv-test-record-window ()
  (push (list :buffer (buffer-name)
              :selected (eq (selected-window) lv-wnd)
              :live (window-live-p lv-wnd))
        lv-test-hook-events))

(defun lv-test-reset ()
  (when (window-live-p lv-wnd)
    (lv-delete-window))
  (when-let ((buffer (get-buffer " *LV*")))
    (kill-buffer buffer))
  (setq lv-wnd nil
        lv-use-separator nil
        lv-use-padding nil
        lv-force-update nil
        lv-window-hook nil
        lv-test-hook-events nil))

(defun lv-test-leading-spaces (string)
  (- (length string) (length (string-trim-left string))))
"##;

fn lv_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LV_MELPA_PIN, "lv.el")
        .expect("prepare pinned lv source below ./tmp")
        .with_prelude(LV_TEST_PRELUDE)
        .with_timeout(LV_TEST_TIMEOUT)
}

fn deployment_hint_creates_a_dedicated_window_without_stealing_selection() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (lv-test-reset)
  (let ((original-window (selected-window))
        (original-buffer (current-buffer))
        result)
    (setq lv-window-hook '(lv-test-record-window))
    (unwind-protect
        (progn
          (lv-message "Deploy %s\n%s" "REL-417" "s: stage  q: quit")
          (let ((first-window lv-wnd))
            (lv-window)
            (setq result
                  (list
                   :selection-restored
                   (and (eq (selected-window) original-window)
                        (eq (current-buffer) original-buffer))
                   :same-window (eq first-window lv-wnd)
                   :ordinary-windows (length (window-list nil 'nomini))
                   :hook-events (nreverse lv-test-hook-events)
                   :window
                   (list :live (window-live-p lv-wnd)
                         :buffer (buffer-name (window-buffer lv-wnd))
                         :dedicated (window-dedicated-p lv-wnd)
                         :no-other-window
                         (window-parameter lv-wnd 'no-other-window)
                         :hscroll (window-hscroll lv-wnd))
                   :buffer
                   (with-current-buffer (window-buffer lv-wnd)
                     (list :content
                           (buffer-substring-no-properties
                            (point-min) (point-max))
                           :mode major-mode
                           :mode-line mode-line-format
                           :header-line header-line-format
                           :tab-line tab-line-format
                           :cursor cursor-type
                           :line-numbers display-line-numbers
                           :fill-column-indicator
                           display-fill-column-indicator
                           :size-fixed window-size-fixed
                           :min-height window-min-height
                           :truncate truncate-lines
                           :point (point)))))))
      (lv-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:selection-restored t :same-window t :ordinary-windows 2 :hook-events ((:buffer " *LV*" :selected t :live t)) :window (:live t :buffer " *LV*" :dedicated t :no-other-window t :hscroll 0) :buffer (:content "Deploy REL-417\ns: stage  q: quit" :mode fundamental-mode :mode-line nil :header-line nil :tab-line nil :cursor nil :line-numbers nil :fill-column-indicator nil :size-fixed t :min-height 1 :truncate nil :point 1))"##
    ]];
    ParityBatchCase::value(
        "deployment_hint_creates_a_dedicated_window_without_stealing_selection",
        elisp_form,
        expect,
    )
}

fn repeated_hint_is_suppressed_unless_forced_then_updates_multiline_state() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (lv-test-reset)
  (let (result)
    (unwind-protect
        (progn
          (lv-message "Release %s ready" "REL-417")
          (let* ((window lv-wnd)
                 (buffer (window-buffer window))
                 (initial-tick
                  (with-current-buffer buffer (buffer-chars-modified-tick))))
            (lv-message "Release %s ready" "REL-417")
            (let ((suppressed-tick
                   (with-current-buffer buffer (buffer-chars-modified-tick))))
              (let ((lv-force-update t))
                (lv-message "Release %s ready" "REL-417"))
              (let ((forced-tick
                     (with-current-buffer buffer (buffer-chars-modified-tick))))
                (lv-message "Release %s\n%s\n%s"
                            "REL-418" "v: verify" "q: quit")
                (setq result
                      (list
                       :same-window (eq window lv-wnd)
                       :ticks
                       (list :suppressed (= initial-tick suppressed-tick)
                             :forced (> forced-tick suppressed-tick))
                       :buffer
                       (with-current-buffer buffer
                         (list :content
                               (buffer-substring-no-properties
                                (point-min) (point-max))
                               :min-height window-min-height
                               :truncate truncate-lines
                               :point (point)))
                       :selection-restored
                       (not (eq (selected-window) lv-wnd))))))))
      (lv-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:same-window t :ticks (:suppressed t :forced t) :buffer (:content "Release REL-418\nv: verify\nq: quit" :min-height 2 :truncate t :point 1) :selection-restored t)"##
    ]];
    ParityBatchCase::value(
        "repeated_hint_is_suppressed_unless_forced_then_updates_multiline_state",
        elisp_form,
        expect,
    )
}

fn padded_multiline_hint_centers_the_workflow_using_the_first_line() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (lv-test-reset)
  (let (result)
    (unwind-protect
        (progn
          (setq lv-use-padding t)
          (let* ((window (lv-window))
                 (width (window-width window))
                 (headline "Deploy REL-417")
                 (command "q: quit")
                 (expected-padding (/ (- width (length headline)) 2)))
            (lv-message "%s\n%s" headline command)
            (let* ((content
                    (with-current-buffer (window-buffer lv-wnd)
                      (buffer-substring-no-properties
                       (point-min) (point-max))))
                   (lines (split-string content "\n")))
              (setq result
                    (list
                     :width width
                     :expected-padding expected-padding
                     :lines
                     (mapcar
                      (lambda (line)
                        (list :leading (lv-test-leading-spaces line)
                              :text (string-trim-left line)
                              :length (length line)))
                      lines)
                     :same-padding
                     (= (lv-test-leading-spaces (car lines))
                        (lv-test-leading-spaces (cadr lines))))))))
      (lv-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:width 80 :expected-padding 33 :lines ((:leading 33 :text "Deploy REL-417" :length 47) (:leading 33 :text "q: quit" :length 40)) :same-padding t)"##
    ]];
    ParityBatchCase::value(
        "padded_multiline_hint_centers_the_workflow_using_the_first_line",
        elisp_form,
        expect,
    )
}

fn graphical_separator_appends_the_exact_display_and_face_properties() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (lv-test-reset)
  (let (result)
    (unwind-protect
        (let ((lv-use-separator t))
          (cl-letf (((symbol-function 'window-system)
                     (lambda (&optional _frame) t)))
            (lv-message "Continue deployment?")
            (with-current-buffer (window-buffer lv-wnd)
              (goto-char (point-min))
              (search-forward "__")
              (let ((separator-start (match-beginning 0))
                    (separator-end (match-end 0)))
                (setq result
                      (list
                       :content
                       (buffer-substring-no-properties
                        (point-min) (point-max))
                       :separator
                       (list
                        :first-face
                        (get-text-property separator-start 'face)
                        :first-display
                        (get-text-property separator-start 'display)
                        :second-face
                        (get-text-property (1- separator-end) 'face)
                        :second-display
                        (get-text-property (1- separator-end) 'display))
                       :newline
                       (list
                        :face (get-text-property separator-end 'face)
                        :line-height
                        (get-text-property separator-end 'line-height))
                       :min-height window-min-height
                       :truncate truncate-lines))))))
      (lv-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:content "Continue deployment?\n__\n" :separator (:first-face lv-separator :first-display #1=(space :height (1)) :second-face lv-separator :second-display #1#) :newline (:face lv-separator :line-height t) :min-height 0 :truncate nil)"##
    ]];
    ParityBatchCase::value(
        "graphical_separator_appends_the_exact_display_and_face_properties",
        elisp_form,
        expect,
    )
}

fn formatting_failure_leaves_the_existing_hint_and_window_untouched() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (lv-test-reset)
  (let ((original-window (selected-window))
        result)
    (unwind-protect
        (progn
          (lv-message "Release %s ready" "REL-417")
          (let* ((window lv-wnd)
                 (buffer (window-buffer window))
                 (before-content
                  (with-current-buffer buffer
                    (buffer-substring-no-properties
                     (point-min) (point-max))))
                 (before-tick
                  (with-current-buffer buffer
                    (buffer-chars-modified-tick)))
                 (failure
                  (condition-case problem
                      (progn
                        (lv-message "attempt=%d" "three")
                        :not-signaled)
                    (error
                     (list (car problem)
                           (error-message-string problem))))))
            (setq result
                  (list
                   :failure failure
                   :same-window (eq window lv-wnd)
                   :live (window-live-p window)
                   :selection-restored
                   (eq (selected-window) original-window)
                   :content-unchanged
                   (equal before-content
                          (with-current-buffer buffer
                            (buffer-substring-no-properties
                             (point-min) (point-max))))
                   :tick-unchanged
                   (= before-tick
                      (with-current-buffer buffer
                        (buffer-chars-modified-tick)))
                   :content before-content))))
      (lv-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:failure (error "Format specifier doesn’t match argument type") :same-window t :live t :selection-restored t :content-unchanged t :tick-unchanged t :content "Release REL-417 ready")"##
    ]];
    ParityBatchCase::value(
        "formatting_failure_leaves_the_existing_hint_and_window_untouched",
        elisp_form,
        expect,
    )
}

fn deleting_and_recreating_the_hint_rebuilds_both_window_and_buffer() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (lv-test-reset)
  (let ((original-window (selected-window))
        result)
    (setq lv-window-hook '(lv-test-record-window))
    (unwind-protect
        (progn
          (lv-message "Release %s staged" "REL-417")
          (let ((old-window lv-wnd)
                (old-buffer (window-buffer lv-wnd)))
            (lv-delete-window)
            (let ((deleted
                   (list :window-live (window-live-p old-window)
                         :buffer-live (buffer-live-p old-buffer)
                         :lv-reference-same (eq lv-wnd old-window)
                         :ordinary-windows
                         (length (window-list nil 'nomini)))))
              (lv-message "Release %s verified" "REL-418")
              (setq result
                    (list
                     :deleted deleted
                     :recreated
                     (list :new-window (not (eq lv-wnd old-window))
                           :new-buffer
                           (not (eq (window-buffer lv-wnd) old-buffer))
                           :content
                           (with-current-buffer (window-buffer lv-wnd)
                             (buffer-substring-no-properties
                              (point-min) (point-max)))
                           :selected-original
                           (eq (selected-window) original-window))
                     :hook-events (nreverse lv-test-hook-events))))))
      (lv-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:deleted (:window-live nil :buffer-live nil :lv-reference-same t :ordinary-windows 1) :recreated (:new-window t :new-buffer t :content "Release REL-418 verified" :selected-original t) :hook-events ((:buffer " *LV*" :selected t :live t) (:buffer " *LV*" :selected t :live t)))"##
    ]];
    ParityBatchCase::value(
        "deleting_and_recreating_the_hint_rebuilds_both_window_and_buffer",
        elisp_form,
        expect,
    )
}

fn a_preexisting_lv_buffer_is_adopted_without_reinitializing_its_ui_state() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (lv-test-reset)
  (let ((buffer (get-buffer-create " *LV*"))
        result)
    (with-current-buffer buffer
      (text-mode)
      (setq-local mode-line-format "CUSTOM STATUS")
      (setq-local header-line-format "Pinned environment")
      (setq-local cursor-type 'box)
      (insert "stale hint"))
    (setq lv-window-hook '(lv-test-record-window))
    (unwind-protect
        (progn
          (lv-message "Deploy %s" "REL-419")
          (setq result
                (list
                 :same-buffer (eq buffer (window-buffer lv-wnd))
                 :content
                 (with-current-buffer buffer
                   (buffer-substring-no-properties
                    (point-min) (point-max)))
                 :ui-state
                 (with-current-buffer buffer
                   (list :mode major-mode
                         :mode-line mode-line-format
                         :header-line header-line-format
                         :cursor cursor-type))
                 :window
                 (list :dedicated (window-dedicated-p lv-wnd)
                       :no-other-window
                       (window-parameter lv-wnd 'no-other-window))
                 :hook-events lv-test-hook-events)))
      (lv-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:same-buffer t :content "Deploy REL-419" :ui-state (:mode text-mode :mode-line "CUSTOM STATUS" :header-line "Pinned environment" :cursor box) :window (:dedicated nil :no-other-window nil) :hook-events nil)"##
    ]];
    ParityBatchCase::value(
        "a_preexisting_lv_buffer_is_adopted_without_reinitializing_its_ui_state",
        elisp_form,
        expect,
    )
}

#[test]
fn lv_package_batch() {
    let cases = vec![
        deployment_hint_creates_a_dedicated_window_without_stealing_selection(),
        repeated_hint_is_suppressed_unless_forced_then_updates_multiline_state(),
        padded_multiline_hint_centers_the_workflow_using_the_first_line(),
        graphical_separator_appends_the_exact_display_and_face_properties(),
        formatting_failure_leaves_the_existing_hint_and_window_untouched(),
        deleting_and_recreating_the_hint_rebuilds_both_window_and_buffer(),
        a_preexisting_lv_buffer_is_adopted_without_reinitializing_its_ui_state(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed lv parity test");
    assert_oracle_batch_cases(lv_oracle(), test_name, "lv_parity", &cases);
}
