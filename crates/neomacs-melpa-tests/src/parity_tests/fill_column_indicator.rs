use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FILL_COLUMN_INDICATOR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'fill-column-indicator)

(defun neomacs-fci-test-call-with-buffer (name text function)
  "Display a temporary buffer containing TEXT and call FUNCTION there."
  (save-window-excursion
    (when-let ((existing (get-buffer name)))
      (kill-buffer existing))
    (let ((buffer (generate-new-buffer name)))
      (unwind-protect
          (progn
            (set-window-buffer (selected-window) buffer)
            (with-current-buffer buffer
              (insert text)
              (goto-char (point-min))
              (set-window-start (selected-window) (point-min))
              (setq-local fci-always-use-textual-rule t)
              (setq-local fci-rule-color "white")
              (funcall function)))
        (when (buffer-live-p buffer)
          (with-current-buffer buffer
            (when fci-mode
              (fci-mode 0)))
          (kill-buffer buffer))))))

(defun neomacs-fci-test-overlays ()
  "Describe every FCI overlay in buffer order."
  (mapcar
   (lambda (overlay)
     (save-excursion
       (goto-char (overlay-start overlay))
       (let ((after-string (overlay-get overlay 'after-string)))
         (list :line (line-number-at-pos)
               :position (overlay-start overlay)
               :column (current-column)
               :kind (cond
                      ((equal after-string fci-pre-limit-string) 'pre)
                      ((equal after-string fci-at-limit-string) 'at)
                      ((equal after-string fci-post-limit-string) 'post)
                      (t 'unknown))
               :codes (and after-string (string-to-list after-string))))))
   (sort (fci-get-overlays-region (point-min) (point-max))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun neomacs-fci-test-string-signature (string)
  "Describe STRING's characters and display properties exactly."
  (cl-loop for index below (length string)
           collect
           (list :character (aref string index)
                 :cursor (get-text-property index 'cursor string)
                 :display (get-text-property index 'display string))))

(defun neomacs-fci-test-mode-state ()
  "Describe FCI's effective rule and display-table state."
  (list :mode fci-mode
        :column fci-column
        :limit fci-limit
        :tab-width fci-tab-width
        :newline fci-newline
        :display-newline (and buffer-display-table
                              (aref buffer-display-table ?\n))
        :display-eol (and buffer-display-table
                          (aref buffer-display-table fci-eol-char))
        :display-blank (and buffer-display-table
                            (aref buffer-display-table fci-blank-char))
        :overlays (neomacs-fci-test-overlays)))

(defun neomacs-fci-test-hook-present-p (hook function)
  "Return t when FUNCTION is present in buffer-local HOOK."
  (and (memq function (symbol-value hook)) t))
"####;

fn rule_placement_distinguishes_short_exact_and_over_limit_lines() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-fci-test-call-with-buffer
 "*fci-release-policy*"
 "short\nabcdefghijkl\nabcdefghijklmnop\ntail\n"
 (lambda ()
   (setq-local fill-column 12)
   (fci-mode 1)
   (list :state (neomacs-fci-test-mode-state)
         :strings
         (list :pre (neomacs-fci-test-string-signature
                     fci-pre-limit-string)
               :at (neomacs-fci-test-string-signature
                    fci-at-limit-string)
               :post (neomacs-fci-test-string-signature
                      fci-post-limit-string)))))
"####;
    let expected = expect![[
        r#"OK (:state (:mode t :column 12 :limit 12 :tab-width 8 :newline nil :display-newline nil :display-eol [32] :display-blank [32] :overlays ((:line 1 :position 6 :column 5 :kind pre :codes (57344 57345 57345)) (:line 2 :position 19 :column 12 :kind at :codes (57345)) (:line 3 :position 36 :column 16 :kind post :codes (57344 57345)) (:line 4 :position 41 :column 4 :kind pre :codes (57344 57345 57345)))) :strings (:pre ((:character 57344 :cursor t :display #("" 0 1 (cursor t))) (:character 57345 :cursor nil :display ((when (not (fci-competing-overlay-p buffer-position)) space :align-to fci-column) (space :width 0))) (:character 57345 :cursor nil :display ((when #1=(not (fci-competing-overlay-p buffer-position)) . #("|" 0 1 (cursor nil face #2=(:foreground "white" :weight normal :slant normal)))) . #3=((space :width 0))))) :at ((:character 57345 :cursor t :display ((when #1# . #("|" 0 1 (cursor t face #2#))) . #3#))) :post ((:character 57344 :cursor t :display #("" 0 1 (cursor t))) (:character 57345 :cursor nil :display (space :width 0)))))"#
    ]];
    ParityBatchCase::value(
        "rule_placement_distinguishes_short_exact_and_over_limit_lines",
        elisp_form,
        expected,
    )
}

fn live_edits_redraw_changed_lines_without_duplicate_or_stale_rules() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-fci-test-call-with-buffer
 "*fci-live-edit*"
 "short\nabcdefghijkl\nlonger-than-policy\n"
 (lambda ()
   (setq-local fill-column 12)
   (fci-mode 1)
   (let ((initial (neomacs-fci-test-overlays)))
     (goto-char (point-min))
     (end-of-line)
     (insert " plus verified")
     (let ((after-expansion (neomacs-fci-test-overlays)))
       (goto-char (point-max))
       (insert "queued\n")
       (let ((after-append (neomacs-fci-test-overlays)))
         (goto-char (point-min))
         (forward-line 1)
         (let ((start (point)))
           (forward-line 1)
           (delete-region start (point)))
         (list :text (buffer-string)
               :initial initial
               :after-expansion after-expansion
               :after-append after-append
               :after-delete (neomacs-fci-test-overlays)
               :newline-count (cl-count ?\n (buffer-string))))))))
"####;
    let expected = expect![[
        r#"OK (:text "short plus verified\nlonger-than-policy\nqueued\n" :initial ((:line 1 :position 6 :column 5 :kind pre :codes (57344 57345 57345)) (:line 2 :position 19 :column 12 :kind at :codes (57345)) (:line 3 :position 38 :column 18 :kind post :codes (57344 57345))) :after-expansion ((:line 1 :position 20 :column 19 :kind post :codes (57344 57345)) (:line 2 :position 33 :column 12 :kind at :codes (57345)) (:line 3 :position 52 :column 18 :kind post :codes (57344 57345))) :after-append ((:line 1 :position 20 :column 19 :kind post :codes (57344 57345)) (:line 2 :position 33 :column 12 :kind at :codes (57345)) (:line 3 :position 52 :column 18 :kind post :codes (57344 57345)) (:line 4 :position 59 :column 6 :kind pre :codes (57344 57345 57345))) :after-delete ((:line 1 :position 20 :column 19 :kind post :codes (57344 57345)) (:line 2 :position 39 :column 18 :kind post :codes (57344 57345)) (:line 3 :position 46 :column 6 :kind pre :codes (57344 57345 57345))) :newline-count 3)"#
    ]];
    ParityBatchCase::value(
        "live_edits_redraw_changed_lines_without_duplicate_or_stale_rules",
        elisp_form,
        expected,
    )
}

fn tab_width_and_policy_changes_recalculate_real_visual_columns() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-fci-test-call-with-buffer
 "*fci-tabbed-config*"
 "\tjob\n\t1234\n\t\tready\n"
 (lambda ()
   (setq-local fill-column 70)
   (setq-local fci-rule-column 8)
   (setq-local tab-width 4)
   (fci-mode 1)
   (let ((tab-four (neomacs-fci-test-mode-state)))
     (setq tab-width 8)
     (fci-post-command-check)
     (let ((tab-eight (neomacs-fci-test-mode-state)))
       (setq fci-rule-column nil
             fill-column 10)
       (fci-post-command-check)
       (list :tab-four tab-four
             :tab-eight tab-eight
             :fill-column-policy (neomacs-fci-test-mode-state))))))
"####;
    let expected = expect![
        "OK (:tab-four (:mode t :column 8 :limit 8 :tab-width 4 :newline nil :display-newline nil :display-eol #1=[32] :display-blank #2=[32] :overlays ((:line 1 :position 5 :column 7 :kind pre :codes (57344 57345 57345)) (:line 2 :position 11 :column 8 :kind at :codes (57345)) (:line 3 :position 19 :column 13 :kind post :codes (57344 57345)))) :tab-eight (:mode t :column 8 :limit 8 :tab-width 8 :newline nil :display-newline nil :display-eol #1# :display-blank #2# :overlays ((:line 1 :position 5 :column 11 :kind post :codes (57344 57345)) (:line 2 :position 11 :column 12 :kind post :codes (57344 57345)) (:line 3 :position 19 :column 21 :kind post :codes (57344 57345)))) :fill-column-policy (:mode t :column 10 :limit 10 :tab-width 8 :newline nil :display-newline nil :display-eol #1# :display-blank #2# :overlays ((:line 1 :position 5 :column 11 :kind post :codes (57344 57345)) (:line 2 :position 11 :column 12 :kind post :codes (57344 57345)) (:line 3 :position 19 :column 21 :kind post :codes (57344 57345)))))"
    ];
    ParityBatchCase::value(
        "tab_width_and_policy_changes_recalculate_real_visual_columns",
        elisp_form,
        expected,
    )
}

fn an_existing_display_table_is_composed_with_and_restored_after_the_rule() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-fci-test-call-with-buffer
 "*fci-display-table*"
 "short\nabcdefghij\n"
 (lambda ()
   (let ((table (make-display-table)))
     (aset table ?X [?Y])
     (aset table ?\n [?~ ?\n])
     (setq buffer-display-table table)
     (setq-local fill-column 10)
     (fci-mode 1)
     (let ((enabled
            (list :state (neomacs-fci-test-mode-state)
                  :saved-newline fci-saved-eol
                  :same-table (eq buffer-display-table table)
                  :x-entry (aref buffer-display-table ?X))))
       (fci-mode 0)
       (list :enabled enabled
             :disabled
             (list :mode fci-mode
                   :same-table (eq buffer-display-table table)
                   :newline (aref buffer-display-table ?\n)
                   :x-entry (aref buffer-display-table ?X)
                   :eol-entry (aref buffer-display-table fci-eol-char)
                   :blank-entry (aref buffer-display-table fci-blank-char)
                   :overlays (fci-get-overlays-region
                              (point-min) (point-max))))))))
"####;
    let expected = expect![
        "OK (:enabled (:state (:mode t :column 10 :limit 9 :tab-width 8 :newline #1=[10] :display-newline #1# :display-eol #4=[126] :display-blank #5=[32] :overlays ((:line 1 :position 6 :column 5 :kind pre :codes (57344 57345 57345)) (:line 2 :position 17 :column 10 :kind at :codes (57344 57345)))) :saved-newline #2=[126 10] :same-table t :x-entry #3=[89]) :disabled (:mode nil :same-table t :newline #2# :x-entry #3# :eol-entry #4# :blank-entry #5# :overlays nil))"
    ];
    ParityBatchCase::value(
        "an_existing_display_table_is_composed_with_and_restored_after_the_rule",
        elisp_form,
        expected,
    )
}

fn mode_lifecycle_restores_navigation_settings_and_removes_edit_hooks() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-fci-test-call-with-buffer
 "*fci-lifecycle*"
 "alpha\nbeta\n"
 (lambda ()
   (setq truncate-lines nil)
   (kill-local-variable 'line-move-visual)
   (let ((initial
          (list :truncate truncate-lines
                :line-move line-move-visual
                :line-local (local-variable-p 'line-move-visual))))
     (fci-mode 1)
     (let ((enabled
            (list :truncate truncate-lines
                  :line-move line-move-visual
                  :line-local (local-variable-p 'line-move-visual)
                  :after-change
                  (neomacs-fci-test-hook-present-p
                   'after-change-functions 'fci-redraw-region)
                  :before-change
                  (neomacs-fci-test-hook-present-p
                   'before-change-functions 'fci-extend-rule-for-deletion)
                  :post-command
                  (neomacs-fci-test-hook-present-p
                   'post-command-hook 'fci-post-command-check))))
       (fci-mode 0)
       (let ((restored
              (list :truncate truncate-lines
                    :line-move line-move-visual
                    :line-local (local-variable-p 'line-move-visual)
                    :after-change
                    (neomacs-fci-test-hook-present-p
                     'after-change-functions 'fci-redraw-region)
                    :before-change
                    (neomacs-fci-test-hook-present-p
                     'before-change-functions 'fci-extend-rule-for-deletion)
                    :post-command
                    (neomacs-fci-test-hook-present-p
                     'post-command-hook 'fci-post-command-check)
                    :overlays (fci-get-overlays-region
                               (point-min) (point-max)))))
         (setq-local truncate-lines t)
         (setq-local line-move-visual t)
         (fci-mode 1)
         (fci-mode 0)
         (list :initial initial
               :enabled enabled
               :restored restored
               :preexisting-locals
               (list :truncate truncate-lines
                     :line-move line-move-visual
                     :line-local (local-variable-p 'line-move-visual))))))))
"####;
    let expected = expect![
        "OK (:initial (:truncate nil :line-move t :line-local nil) :enabled (:truncate t :line-move nil :line-local t :after-change t :before-change t :post-command t) :restored (:truncate nil :line-move t :line-local nil :after-change nil :before-change nil :post-command nil :overlays nil) :preexisting-locals (:truncate t :line-move t :line-local t))"
    ];
    ParityBatchCase::value(
        "mode_lifecycle_restores_navigation_settings_and_removes_edit_hooks",
        elisp_form,
        expected,
    )
}

fn competing_overlay_detection_exposes_plist_and_legacy_face_behavior() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-fci-test-call-with-buffer
 "*fci-overlay-competition*"
 "queued\nrunning\nfailed\n"
 (lambda ()
   (setq-local fill-column 10)
   (let (foreground background legacy)
     (goto-char (point-min))
     (end-of-line)
     (setq foreground (make-overlay (1- (point)) (1+ (point))))
     (overlay-put foreground 'face '(:foreground "yellow"))
     (forward-line 1)
     (end-of-line)
     (setq background (make-overlay (1- (point)) (1+ (point))))
     (overlay-put background 'face '(:background "red"))
     (forward-line 1)
     (end-of-line)
     (setq legacy (make-overlay (1- (point)) (1+ (point))))
     (overlay-put legacy 'face '(background-color . "blue"))
     (fci-mode 1)
     (list :competition
           (mapcar
            (lambda (overlay)
              (goto-char (1+ (overlay-start overlay)))
              (list (line-number-at-pos)
                    (and (fci-competing-overlay-p (point)) t)))
            (list foreground background legacy))
           :face-classification
           (mapcar (lambda (overlay)
                     (fci-overlay-fills-background-p overlay))
                   (list foreground background legacy))
           :rules (neomacs-fci-test-overlays)
           :padding-conditional
           (equal (get-text-property 1 'display fci-pre-limit-string)
                  fci-padding-display)))))
"####;
    let expected = expect![
        "OK (:competition ((1 nil) (2 nil) (3 t)) :face-classification (nil (:background \"red\") t) :rules ((:line 1 :position 7 :column 6 :kind pre :codes (57344 57345 57345)) (:line 2 :position 15 :column 7 :kind pre :codes (57344 57345 57345)) (:line 3 :position 22 :column 6 :kind pre :codes (57344 57345 57345))) :padding-conditional t)"
    ];
    ParityBatchCase::value(
        "competing_overlay_detection_exposes_plist_and_legacy_face_behavior",
        elisp_form,
        expected,
    )
}

fn bitmap_rule_generation_honors_width_dash_ratio_and_color() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-fci-test-call-with-buffer
 "*fci-bitmap-rule*"
 ""
 (lambda ()
   ;; FCI's geometry variables are automatically buffer-local.  Assigning
   ;; them in the displayed work buffer mirrors the real mode lifecycle.
   (setq-local fci-char-width 5)
   (setq-local fci-char-height 8)
   (setq-local fci-rule-width 2)
   (setq-local fci-rule-use-dashes t)
   (setq-local fci-dash-pattern 0.5)
   (setq-local fci-rule-color "#123456")
   (let* ((pbm (fci-make-pbm-img))
          (xpm (fci-make-xpm-img))
          (pbm-data (plist-get (cdr pbm) :data))
          (xpm-data (plist-get (cdr xpm) :data)))
     (list :pbm pbm
           :xpm xpm
           :raster-counts
           (list :pbm-on (cl-count ?1 pbm-data)
                 :pbm-off (cl-count ?0 pbm-data)
                 :xpm-on (cl-count ?1 xpm-data)
                 :xpm-off (cl-count ?0 xpm-data))
           :clamped
           (mapcar
            (lambda (ratio)
              (setq fci-dash-pattern ratio)
              (let ((data (plist-get (cdr (fci-make-pbm-img)) :data)))
                (list ratio
                      (cl-count ?1 data)
                      (cl-count ?0 data))))
            '(-1.0 2.0))))))
"####;
    let expected = expect![[
        r##"OK (:pbm (image :type pbm :data "P1\n5 8\n0 0 0 0 0\n0 0 0 0 0\n0 1 1 0 0\n0 1 1 0 0\n0 1 1 0 0\n0 1 1 0 0\n0 0 0 0 0\n0 0 0 0 0" :mask heuristic :foreground "#123456" :ascent center) :xpm (image :type xpm :data "/* XPM */\nstatic char *rule[] = {\"5 8 2 1\",\"1 c #123456\",\"0 c None\",\"00000\",\"00000\",\"01100\",\"01100\",\"01100\",\"01100\",\"00000\",\"00000\",};" :mask heuristic :ascent center) :raster-counts (:pbm-on 9 :pbm-off 32 :xpm-on 11 :xpm-off 33) :clamped ((-1.0 1 40) (2.0 17 24)))"##
    ]];
    ParityBatchCase::value(
        "bitmap_rule_generation_honors_width_dash_ratio_and_color",
        elisp_form,
        expected,
    )
}

fn invalid_policy_rolls_back_cleanly_and_a_corrected_policy_can_enable() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-fci-test-call-with-buffer
 "*fci-invalid-policy*"
 "alpha\nbeta\n"
 (lambda ()
   (setq-local fci-rule-column 0)
   (let ((failure
          (condition-case error-data
              (list :ok (fci-mode 1))
            (error
             (list :error (car error-data)
                   :data (cdr error-data))))))
     (let ((rolled-back
            (list :mode fci-mode
                  :display-table buffer-display-table
                  :truncate truncate-lines
                  :overlays (fci-get-overlays-region
                             (point-min) (point-max))
                  :after-change
                  (neomacs-fci-test-hook-present-p
                   'after-change-functions 'fci-redraw-region))))
       (setq fci-rule-column 6)
       (fci-mode 1)
       (list :failure failure
             :rolled-back rolled-back
             :corrected (neomacs-fci-test-mode-state))))))
"####;
    let expected = expect![
        "OK (:failure (:error wrong-type-argument :data (fci-posint-p 0)) :rolled-back (:mode nil :display-table nil :truncate nil :overlays nil :after-change nil) :corrected (:mode t :column 6 :limit 6 :tab-width 8 :newline nil :display-newline nil :display-eol [32] :display-blank [32] :overlays ((:line 1 :position 6 :column 5 :kind pre :codes (57344 57345 57345)) (:line 2 :position 11 :column 4 :kind pre :codes (57344 57345 57345)))))"
    ];
    ParityBatchCase::value(
        "invalid_policy_rolls_back_cleanly_and_a_corrected_policy_can_enable",
        elisp_form,
        expected,
    )
}

#[test]
fn fill_column_indicator_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(FILL_COLUMN_INDICATOR_MELPA_PIN, "fill-column-indicator.el")
            .expect("prepare revision-pinned Fill Column Indicator source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "fill-column-indicator-package-batch",
        "Fill Column Indicator",
        &[
            rule_placement_distinguishes_short_exact_and_over_limit_lines(),
            live_edits_redraw_changed_lines_without_duplicate_or_stale_rules(),
            tab_width_and_policy_changes_recalculate_real_visual_columns(),
            an_existing_display_table_is_composed_with_and_restored_after_the_rule(),
            mode_lifecycle_restores_navigation_settings_and_removes_edit_hooks(),
            competing_overlay_detection_exposes_plist_and_legacy_face_behavior(),
            bitmap_rule_generation_honors_width_dash_ratio_and_color(),
            invalid_policy_rolls_back_cleanly_and_a_corrected_policy_can_enable(),
        ],
    );
}
