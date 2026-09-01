use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PAGE_BREAK_LINES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PAGE_BREAK_LINES_TEST_TIMEOUT: Duration = Duration::from_secs(30);

const PAGE_BREAK_LINES_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'display-line-numbers)
(require 'page-break-lines)

(define-derived-mode neomacs-page-break-lines-test-base-mode
  fundamental-mode "PBL-Base")
(define-derived-mode neomacs-page-break-lines-test-doc-mode
  neomacs-page-break-lines-test-base-mode "PBL-Doc")
(define-derived-mode neomacs-page-break-lines-test-other-mode
  fundamental-mode "PBL-Other")

(defun neomacs-page-break-lines-test-entry (&optional buffer)
  "Return BUFFER's form-feed display entry without printing its table."
  (with-current-buffer (or buffer (current-buffer))
    (and buffer-display-table
         (aref buffer-display-table ?\^L))))

(defun neomacs-page-break-lines-test-rule-summary (&optional buffer)
  "Describe BUFFER's form-feed rule through stable glyph semantics."
  (with-current-buffer (or buffer (current-buffer))
    (let* ((entry (neomacs-page-break-lines-test-entry))
           (glyphs (and (vectorp entry) (append entry nil))))
      (list
       :table-subtype
       (and (char-table-p buffer-display-table)
            (char-table-subtype buffer-display-table))
       :last-extra-slot
       (and
        (char-table-p buffer-display-table)
        (condition-case nil
            (progn
              (char-table-extra-slot buffer-display-table 17)
              'readable)
          (error 'invalid)))
       :entry-present (and entry t)
       :length (and (vectorp entry) (length entry))
       :characters
       (and (vectorp entry)
            (apply #'string (mapcar #'glyph-char glyphs)))
       :uniform
       (and glyphs
            (cl-every (lambda (glyph) (equal glyph (car glyphs)))
                      (cdr glyphs)))
       :faces
       (and glyphs
            (delete-dups (mapcar #'glyph-face glyphs)))))))

(defun neomacs-page-break-lines-test-face-summary ()
  "Describe the package face without numeric face identifiers."
  (list
   :inherit (face-attribute 'page-break-lines :inherit nil nil)
   :weight (face-attribute 'page-break-lines :weight nil nil)
   :slant (face-attribute 'page-break-lines :slant nil nil)))

(defun neomacs-page-break-lines-test-form-feed-positions ()
  "Return every form-feed position in the current buffer."
  (save-excursion
    (goto-char (point-min))
    (let (positions)
      (while (search-forward "\f" nil t)
        (push (1- (point)) positions))
      (nreverse positions))))

(defun neomacs-page-break-lines-test-call-with-face-restored
    (function &optional report)
  "Call FUNCTION and restore package-face state on every frame.
When REPORT is non-nil, verify the real global height side effect too."
  (let* ((frames (frame-list))
         (frame-heights
          (mapcar
           (lambda (frame)
             (cons frame
                   (face-attribute
                    'page-break-lines :height frame nil)))
           frames))
         (future-height
          (face-attribute 'page-break-lines :height t nil))
         (selected-default-height
          (face-attribute
           'default :height (selected-frame) 'default)))
    (unwind-protect
        (progn
          (when report
            (set-face-attribute 'page-break-lines nil :height 777))
          (let ((sentinel-installed
                 (and
                  report
                  (cl-every
                   (lambda (frame)
                     (equal
                      (face-attribute
                       'page-break-lines :height frame nil)
                      777))
                   frames)
                  (equal
                   (face-attribute 'page-break-lines :height t nil)
                   777)))
                (value (funcall function)))
            (if report
                (list
                 :value value
                 :face-height-side-effect
                 (list
                  :sentinel-installed sentinel-installed
                  :existing-frames-match-selected-default
                  (cl-every
                   (lambda (frame)
                     (equal
                      (face-attribute
                       'page-break-lines :height frame nil)
                      selected-default-height))
                   frames)
                  :future-default-matches-selected-default
                  (equal
                   (face-attribute 'page-break-lines :height t nil)
                   selected-default-height)))
              value)))
      (set-face-attribute
       'page-break-lines t :height future-height)
      (dolist (entry frame-heights)
        (when (frame-live-p (car entry))
          (set-face-attribute
           'page-break-lines (car entry) :height (cdr entry)))))))
"####;

fn page_break_lines_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PAGE_BREAK_LINES_MELPA_PIN, "page-break-lines.el")
        .expect("prepare exact page-break-lines source below ./tmp")
        .with_prelude(PAGE_BREAK_LINES_TEST_PRELUDE)
        .with_timeout(PAGE_BREAK_LINES_TEST_TIMEOUT)
}

fn local_mode_preserves_real_pages_while_installing_a_semantic_rule() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-page-break-lines-test-call-with-face-restored
 (lambda ()
   (save-window-excursion
     (let ((buffer (generate-new-buffer " *page-break-lines-pages*"))
           (page-break-lines-modes
            '(neomacs-page-break-lines-test-base-mode))
           (page-break-lines-char ?=)
           (page-break-lines-max-width 18)
           enabled navigation disabled window original-window-table)
       (unwind-protect
           (progn
             (global-page-break-lines-mode -1)
             (delete-other-windows)
             (setq window (selected-window)
                   original-window-table (window-display-table window))
             (set-window-display-table window nil)
             (set-window-buffer window buffer)
             (with-current-buffer buffer
               (neomacs-page-break-lines-test-doc-mode)
               (insert
                "Release 4.2\n"
                "Overview\n"
                "\f\n"
                "Compatibility\n"
                "- GNU Emacs\n"
                "- Neomacs\n"
                "\f\n"
                "Deployment checklist\n")
               (set-buffer-modified-p nil)
               (page-break-lines-mode 1)
               (setq enabled
                     (list
                      :mode page-break-lines-mode
                      :lighter page-break-lines-lighter
                      :text (buffer-string)
                      :form-feeds
                      (neomacs-page-break-lines-test-form-feed-positions)
                      :rule (neomacs-page-break-lines-test-rule-summary)
                      :face (neomacs-page-break-lines-test-face-summary)
                      :window-table-clear
                      (null (window-display-table window))
                      :modified (buffer-modified-p)))
               (goto-char (point-min))
               (let ((start (list (point) (line-number-at-pos))))
                 (forward-page 1)
                 (let ((first (list (point) (line-number-at-pos)
                                    (char-before) (char-after))))
                   (forward-page 1)
                   (let ((second (list (point) (line-number-at-pos)
                                       (char-before) (char-after))))
                     (backward-page 1)
                     (setq navigation
                           (list :start start
                                 :after-first first
                                 :after-second second
                                 :after-back
                                 (list (point) (line-number-at-pos)
                                       (char-before) (char-after)))))))
               (page-break-lines-mode -1)
               (setq disabled
                     (list
                      :mode page-break-lines-mode
                      :rule (neomacs-page-break-lines-test-rule-summary)
                      :text (buffer-string)
                      :form-feeds
                      (neomacs-page-break-lines-test-form-feed-positions)
                      :modified (buffer-modified-p))))
             (list :enabled enabled
                   :navigation navigation
                   :disabled disabled))
         (global-page-break-lines-mode -1)
         (when (window-live-p window)
           (set-window-display-table window original-window-table))
         (when (buffer-live-p buffer)
           (kill-buffer buffer))))))
 t)
"####;
    let expected = expect![[
        r#"OK (:value (:enabled (:mode t :lighter " PgLn" :text "Release 4.2\nOverview\n\f\nCompatibility\n- GNU Emacs\n- Neomacs\n\f\nDeployment checklist\n" :form-feeds (22 60) :rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 18 :characters "==================" :uniform t :faces (page-break-lines)) :face (:inherit font-lock-comment-face :weight normal :slant normal) :window-table-clear t :modified nil) :navigation (:start (1 1) :after-first (23 3 12 10) :after-second (61 7 12 10) :after-back (23 3 12 10)) :disabled (:mode nil :rule (:table-subtype display-table :last-extra-slot readable :entry-present nil :length nil :characters nil :uniform nil :faces nil) :text "Release 4.2\nOverview\n\f\nCompatibility\n- GNU Emacs\n- Neomacs\n\f\nDeployment checklist\n" :form-feeds (22 60) :modified nil)) :face-height-side-effect (:sentinel-installed t :existing-frames-match-selected-default t :future-default-matches-selected-default t))"#
    ]];
    ParityBatchCase::value(
        "local_mode_preserves_real_pages_while_installing_a_semantic_rule",
        elisp_form,
        expected,
    )
}

fn display_table_composition_exposes_exact_ownership_on_disable() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-page-break-lines-test-call-with-face-restored
 (lambda ()
   (save-window-excursion
     (let ((eligible (generate-new-buffer " *page-break-lines-owned*"))
           (excluded (generate-new-buffer " *page-break-lines-excluded*"))
           (page-break-lines-modes
            '(neomacs-page-break-lines-test-base-mode))
           (page-break-lines-char ?=)
           (page-break-lines-max-width 6)
           eligible-state excluded-state)
       (unwind-protect
           (progn
             (global-page-break-lines-mode -1)
             (delete-other-windows)
             (let ((table (make-display-table))
                   (window-table (make-display-table))
                   (window (selected-window))
                   (original-window-table
                    (window-display-table (selected-window)))
                   (original-form-feed [?!]))
               (unwind-protect
                   (progn
                     (aset table ?X [?Y])
                     (aset table ?\^L original-form-feed)
                     (aset window-table ?\^L [?@])
                     (set-window-display-table window window-table)
                     (with-current-buffer eligible
                       (neomacs-page-break-lines-test-doc-mode)
                       (insert "Owned display table\n\f\nNext page\n")
                       (setq buffer-display-table table))
                     (set-window-buffer window eligible)
                     (with-current-buffer eligible
                       (page-break-lines-mode 1)
                       (let ((enabled
                              (list
                               :same-table (eq buffer-display-table table)
                               :unrelated-preserved
                               (equal (aref buffer-display-table ?X) [?Y])
                               :original-replaced
                               (not (equal
                                     (aref buffer-display-table ?\^L)
                                     original-form-feed))
                               :window-override-preserved
                               (and
                                (eq (window-display-table window)
                                    window-table)
                                (equal
                                 (aref (window-display-table window) ?\^L)
                                 [?@]))
                               :rule
                               (neomacs-page-break-lines-test-rule-summary))))
                         (page-break-lines-mode -1)
                         (setq eligible-state
                               (list
                                :enabled enabled
                                :disabled
                                (list
                                 :mode page-break-lines-mode
                                 :same-table (eq buffer-display-table table)
                                 :unrelated-preserved
                                 (equal (aref buffer-display-table ?X) [?Y])
                                 :form-feed (aref buffer-display-table ?\^L)
                                 :original-restored
                                 (equal (aref buffer-display-table ?\^L)
                                        original-form-feed)
                                 :window-override-preserved
                                 (and
                                  (eq (window-display-table window)
                                      window-table)
                                  (equal
                                   (aref (window-display-table window) ?\^L)
                                   [?@]))))))))
                 (set-window-display-table window original-window-table)))
             (let ((table (make-display-table)))
               (aset table ?X [?Z])
               (aset table ?\^L [?#])
               (with-current-buffer excluded
                 (neomacs-page-break-lines-test-other-mode)
                 (insert "Manual mode\n\f\nStill manual\n")
                 (setq buffer-display-table table))
               (set-window-buffer (selected-window) excluded)
               (with-current-buffer excluded
                 (page-break-lines-mode 1)
                 (let ((enabled
                        (list
                         :mode page-break-lines-mode
                         :same-table (eq buffer-display-table table)
                         :rule
                         (neomacs-page-break-lines-test-rule-summary))))
                   (page-break-lines-mode -1)
                   (setq excluded-state
                         (list
                          :enabled enabled
                          :disabled
                          (list
                           :mode page-break-lines-mode
                           :same-table (eq buffer-display-table table)
                           :unrelated-preserved
                           (equal (aref buffer-display-table ?X) [?Z])
                           :rule-remains
                           (neomacs-page-break-lines-test-rule-summary)))))))
             (list :eligible eligible-state
                   :manually-enabled-excluded excluded-state))
         (global-page-break-lines-mode -1)
         (when (buffer-live-p eligible)
           (kill-buffer eligible))
         (when (buffer-live-p excluded)
           (kill-buffer excluded)))))))
"####;
    let expected = expect![[
        r#"OK (:eligible (:enabled (:same-table t :unrelated-preserved t :original-replaced t :window-override-preserved t :rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 6 :characters "======" :uniform t :faces (page-break-lines))) :disabled (:mode nil :same-table t :unrelated-preserved t :form-feed nil :original-restored nil :window-override-preserved t)) :manually-enabled-excluded (:enabled (:mode t :same-table t :rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 6 :characters "======" :uniform t :faces (page-break-lines))) :disabled (:mode nil :same-table t :unrelated-preserved t :rule-remains (:table-subtype display-table :last-extra-slot readable :entry-present t :length 6 :characters "======" :uniform t :faces (page-break-lines)))))"#
    ]];
    ParityBatchCase::value(
        "display_table_composition_exposes_exact_ownership_on_disable",
        elisp_form,
        expected,
    )
}

fn window_hooks_preserve_unselected_buffer_and_window_points() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-page-break-lines-test-call-with-face-restored
 (lambda ()
   (save-current-buffer
     (save-window-excursion
       (let ((first (generate-new-buffer " *page-break-lines-window-a*"))
             (second (generate-new-buffer " *page-break-lines-window-b*"))
             (work (generate-new-buffer " *page-break-lines-work*"))
             (page-break-lines-modes
              '(neomacs-page-break-lines-test-base-mode))
             (page-break-lines-char ?a)
             (page-break-lines-max-width nil))
         (unwind-protect
             (progn
               (global-page-break-lines-mode -1)
               (delete-other-windows)
               (let* ((first-window (selected-window))
                      (second-window (split-window-right))
                      (work-window (split-window first-window nil 'below))
                      (first-window-table
                       (window-display-table first-window))
                      (second-window-table
                       (window-display-table second-window))
                      (widths (list (cons first-window 20)
                                    (cons second-window 30)))
                      selected-before current-before
                      first-point second-point work-point
                      first-window-point second-window-point
                      first-entry-before second-entry-before
                      first-rule-before second-rule-before
                      after-window-config first-entry-after second-entry-after)
                 (unwind-protect
                     (cl-letf
                         (((symbol-function 'window-max-chars-per-line)
                           (lambda (window)
                             (cdr (assq window widths))))
                          ((symbol-function 'string-pixel-width)
                           (lambda (_string &optional _buffer) 100)))
                       (set-window-display-table first-window nil)
                       (set-window-display-table second-window nil)
                       (with-current-buffer first
                         (neomacs-page-break-lines-test-doc-mode)
                         (insert "Alpha page\n\f\nAlpha details\n"))
                       (with-current-buffer second
                         (neomacs-page-break-lines-test-doc-mode)
                         (insert "Beta page\n\f\nBeta details\n")
                         (setq-local page-break-lines-max-width 12))
                       (with-current-buffer work
                         (insert "temporary calculation")
                         (goto-char 3))
                       (set-window-buffer first-window first)
                       (set-window-buffer second-window second)
                       (set-window-buffer work-window work)
                       (select-window work-window)
                       (with-current-buffer first
                         (page-break-lines-mode 1))
                       (with-current-buffer second
                         (page-break-lines-mode 1))

                       ;; Deliberately make each buffer point differ from its
                       ;; window point, then use a third current buffer.  The
                       ;; pre-e0b59f4 `with-selected-window' implementation reset
                       ;; these points while updating display tables.
                       (with-current-buffer first (goto-char 18))
                       (with-current-buffer second (goto-char 19))
                       (set-window-point first-window 4)
                       (set-window-point second-window 8)
                       (select-window work-window)
                       (set-buffer work)
                       (setq selected-before (selected-window)
                             current-before (current-buffer)
                             first-point (with-current-buffer first (point))
                             second-point (with-current-buffer second (point))
                             work-point (point)
                             first-window-point (window-point first-window)
                             second-window-point (window-point second-window)
                             first-entry-before
                             (neomacs-page-break-lines-test-entry first)
                             second-entry-before
                             (neomacs-page-break-lines-test-entry second)
                             first-rule-before
                             (neomacs-page-break-lines-test-rule-summary first)
                             second-rule-before
                             (neomacs-page-break-lines-test-rule-summary second))

                       (let ((window-configuration-change-hook
                              '(page-break-lines--update-display-tables)))
                         (run-hooks 'window-configuration-change-hook))
                       (setq after-window-config
                             (list
                              :selected-window-preserved
                              (eq selected-before (selected-window))
                              :current-buffer-preserved
                              (eq current-before (current-buffer))
                              :buffer-points
                              (list
                               (with-current-buffer first (point))
                               (with-current-buffer second (point))
                               (point))
                              :window-points
                              (list (window-point first-window)
                                    (window-point second-window))
                              :second-entry-reused
                              (eq second-entry-before
                                  (neomacs-page-break-lines-test-entry
                                   second))))

                       (setcdr (assq first-window widths) 24)
                       (let ((window-size-change-functions
                              '(page-break-lines--update-display-tables)))
                         (run-hook-with-args
                          'window-size-change-functions
                          (selected-frame)))
                       (setq first-entry-after
                             (neomacs-page-break-lines-test-entry first)
                             second-entry-after
                             (neomacs-page-break-lines-test-entry second))
                       (list
                        :before
                        (list
                         :first-rule first-rule-before
                         :first-length (length first-entry-before)
                         :second-rule second-rule-before
                         :second-length (length second-entry-before)
                         :window-tables-clear
                         (list (null (window-display-table first-window))
                               (null (window-display-table second-window)))
                         :buffer-points (list first-point second-point work-point)
                         :window-points
                         (list first-window-point second-window-point))
                        :after-window-configuration after-window-config
                        :after-window-size
                        (list
                         :first-length (length first-entry-after)
                         :first-growth
                         (- (length first-entry-after)
                            (length first-entry-before))
                         :second-length (length second-entry-after)
                         :second-entry-reused
                         (eq second-entry-before second-entry-after)
                         :selected-window-preserved
                         (eq selected-before (selected-window))
                         :current-buffer-preserved
                         (eq current-before (current-buffer))
                         :buffer-points-preserved
                         (list
                          (= first-point (with-current-buffer first (point)))
                          (= second-point (with-current-buffer second (point)))
                          (= work-point (point)))
                         :window-points-preserved
                         (list
                          (= first-window-point (window-point first-window))
                          (= second-window-point
                             (window-point second-window))))))
                   (when (window-live-p first-window)
                     (set-window-display-table
                      first-window first-window-table))
                   (when (window-live-p second-window)
                     (set-window-display-table
                      second-window second-window-table)))))
           (global-page-break-lines-mode -1)
           (dolist (buffer (list first second work))
             (when (buffer-live-p buffer)
               (kill-buffer buffer)))))))))
"####;
    let expected = expect![[
        r#"OK (:before (:first-rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 20 :characters "aaaaaaaaaaaaaaaaaaaa" :uniform t :faces (page-break-lines)) :first-length 20 :second-rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 12 :characters "aaaaaaaaaaaa" :uniform t :faces (page-break-lines)) :second-length 12 :window-tables-clear (t t) :buffer-points (18 19 3) :window-points (4 8)) :after-window-configuration (:selected-window-preserved t :current-buffer-preserved t :buffer-points (18 19 3) :window-points (4 8) :second-entry-reused t) :after-window-size (:first-length 24 :first-growth 4 :second-length 12 :second-entry-reused t :selected-window-preserved t :current-buffer-preserved t :buffer-points-preserved (t t t) :window-points-preserved (t t)))"#
    ]];
    ParityBatchCase::value(
        "window_hooks_preserve_unselected_buffer_and_window_points",
        elisp_form,
        expected,
    )
}

fn fractional_pixel_width_and_tiny_windows_produce_exact_rule_lengths() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-page-break-lines-test-call-with-face-restored
 (lambda ()
   (save-window-excursion
     (let ((buffer (generate-new-buffer " *page-break-lines-ratio*"))
           (page-break-lines-modes
            '(neomacs-page-break-lines-test-base-mode))
           (page-break-lines-char ?═)
           (page-break-lines-max-width nil)
           (reported-width (list 23))
           (rule-measures 0)
           (baseline-measures 0)
           (window nil)
           (original-window-table nil))
       (unwind-protect
           (progn
             (global-page-break-lines-mode -1)
             (delete-other-windows)
             (setq window (selected-window)
                   original-window-table (window-display-table window))
             (set-window-display-table window nil)
             (set-window-buffer window buffer)
             (cl-letf
                 (((symbol-function 'window-max-chars-per-line)
                   (lambda (_window) (car reported-width)))
                  ((symbol-function 'string-pixel-width)
                   (lambda (string &optional _buffer)
                     (cond
                      ((and (= (length string) 100)
                            (= (aref string 0) ?═))
                       (setq rule-measures (1+ rule-measures))
                       150)
                      ((and (= (length string) 100)
                            (= (aref string 0) ?a))
                       (setq baseline-measures (1+ baseline-measures))
                       100)
                      (t (error "unexpected width probe: %S" string))))))
               (with-current-buffer buffer
                 (neomacs-page-break-lines-test-doc-mode)
                 (insert "Wide rule\n\f\nTiny frame\n")
                 (goto-char 6)
                 (page-break-lines-mode 1)
                 (let ((fractional
                        (list
                         :reported-columns (car reported-width)
                         :rule
                         (neomacs-page-break-lines-test-rule-summary)
                         :window-table-clear
                         (null (window-display-table window))
                         :point (point)
                         :text (buffer-string))))
                   (setcar reported-width -1)
                   (let ((window-size-change-functions
                          '(page-break-lines--update-display-tables)))
                     (run-hook-with-args
                      'window-size-change-functions
                      (selected-frame)))
                   (list
                    :fractional-ratio fractional
                    :tiny-window
                    (list
                     :reported-columns (car reported-width)
                     :rule
                     (neomacs-page-break-lines-test-rule-summary)
                     :point (point)
                     :text (buffer-string))
                    :measurements
                    (list :rule rule-measures
                          :baseline baseline-measures))))))
         (global-page-break-lines-mode -1)
         (when (window-live-p window)
           (set-window-display-table window original-window-table))
         (when (buffer-live-p buffer)
           (kill-buffer buffer)))))))
"####;
    let expected = expect![[
        r#"OK (:fractional-ratio (:reported-columns 23 :rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 15 :characters "═══════════════" :uniform t :faces (page-break-lines)) :window-table-clear t :point 6 :text "Wide rule\n\f\nTiny frame\n") :tiny-window (:reported-columns -1 :rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 0 :characters "" :uniform nil :faces nil) :point 6 :text "Wide rule\n\f\nTiny frame\n") :measurements (:rule 2 :baseline 2))"#
    ]];
    ParityBatchCase::value(
        "fractional_pixel_width_and_tiny_windows_produce_exact_rule_lengths",
        elisp_form,
        expected,
    )
}

fn global_mode_tracks_eligible_derived_and_new_buffers_but_skips_exclusions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-page-break-lines-test-call-with-face-restored
 (lambda ()
   (save-window-excursion
     (let ((visible (generate-new-buffer " *page-break-lines-global-visible*"))
           (hidden (generate-new-buffer " *page-break-lines-global-hidden*"))
           (excluded (generate-new-buffer " *page-break-lines-global-excluded*"))
           (future-eligible
            (generate-new-buffer " *page-break-lines-global-future*"))
           (future-excluded
            (generate-new-buffer " *page-break-lines-global-future-other*"))
           (page-break-lines-modes
            '(neomacs-page-break-lines-test-base-mode))
           (page-break-lines-char ?~)
           (page-break-lines-max-width 5)
           enabled hidden-shown future disabled minibuffer-state
           window original-window-table)
       (unwind-protect
           (progn
             (global-page-break-lines-mode -1)
             (with-current-buffer visible
               (neomacs-page-break-lines-test-doc-mode)
               (insert "Visible\n\f\nPage\n"))
             (with-current-buffer hidden
               (neomacs-page-break-lines-test-doc-mode)
               (insert "Hidden\n\f\nPage\n"))
             (with-current-buffer excluded
               (neomacs-page-break-lines-test-other-mode)
               (insert "Excluded\n\f\nPage\n")
               (setq buffer-display-table (make-display-table))
               (aset buffer-display-table ?\^L [?!]))
             (delete-other-windows)
             (setq window (selected-window)
                   original-window-table (window-display-table window))
             (set-window-display-table window nil)
             (set-window-buffer window visible)
             (global-page-break-lines-mode 1)
             (let ((minibuffer (window-buffer (minibuffer-window))))
               (setq page-break-lines-char ?+)
               (with-current-buffer minibuffer
                 (unwind-protect
                     (let ((major-mode
                            'neomacs-page-break-lines-test-doc-mode))
                       (let ((window-configuration-change-hook
                              '(page-break-lines--update-display-tables)))
                         (run-hooks 'window-configuration-change-hook))
                       (page-break-lines-mode-maybe)
                       (setq minibuffer-state
                             (list
                              :minibuffer (minibufferp)
                              :mode page-break-lines-mode
                              :table (and buffer-display-table t)
                              :visible-rule
                              (neomacs-page-break-lines-test-rule-summary
                               visible))))
                   (when page-break-lines-mode
                     (page-break-lines-mode -1))))
               (setq page-break-lines-char ?~))
             (setq enabled
                   (list
                    :global global-page-break-lines-mode
                    :visible-mode
                    (with-current-buffer visible page-break-lines-mode)
                    :visible-rule
                    (neomacs-page-break-lines-test-rule-summary visible)
                    :visible-window-table-clear
                    (null (window-display-table window))
                    :hidden-mode
                    (with-current-buffer hidden page-break-lines-mode)
                    :hidden-table
                    (with-current-buffer hidden
                      (and buffer-display-table t))
                    :excluded-mode
                    (with-current-buffer excluded page-break-lines-mode)
                    :excluded-custom-entry
                    (with-current-buffer excluded
                      (equal (aref buffer-display-table ?\^L) [?!]))
                    :minibuffer minibuffer-state))
             (set-window-buffer window hidden)
             (let ((window-configuration-change-hook
                    '(page-break-lines--update-display-tables)))
               (run-hooks 'window-configuration-change-hook))
             (setq hidden-shown
                   (neomacs-page-break-lines-test-rule-summary hidden))
             (with-current-buffer future-eligible
               (neomacs-page-break-lines-test-doc-mode)
               (insert "Future\n\f\nEligible\n"))
             (with-current-buffer future-excluded
               (neomacs-page-break-lines-test-other-mode)
               (insert "Future\n\f\nExcluded\n"))
             (setq future
                   (list
                    :eligible-mode
                    (with-current-buffer future-eligible
                      page-break-lines-mode)
                    :eligible-table
                    (with-current-buffer future-eligible
                      (and buffer-display-table t))
                    :excluded-mode
                    (with-current-buffer future-excluded
                      page-break-lines-mode)
                    :excluded-table
                    (with-current-buffer future-excluded
                      (and buffer-display-table t))))
             (global-page-break-lines-mode -1)
             (dolist (buffer (list visible hidden future-eligible excluded))
               (set-window-buffer window buffer)
               (let ((window-configuration-change-hook
                      '(page-break-lines--update-display-tables)))
                 (run-hooks 'window-configuration-change-hook)))
             (setq disabled
                   (list
                    :global global-page-break-lines-mode
                    :local-modes
                    (mapcar
                     (lambda (buffer)
                       (with-current-buffer buffer page-break-lines-mode))
                     (list visible hidden future-eligible future-excluded))
                    :eligible-rules
                    (mapcar
                     #'neomacs-page-break-lines-test-rule-summary
                     (list visible hidden future-eligible))
                    :excluded-custom-entry
                    (with-current-buffer excluded
                      (equal (aref buffer-display-table ?\^L) [?!]))))
             (list :enabled enabled
                   :hidden-when-displayed hidden-shown
                   :future-buffers future
                   :disabled disabled))
         (global-page-break-lines-mode -1)
         (when (window-live-p window)
           (set-window-display-table window original-window-table))
         (dolist (buffer
                  (list visible hidden excluded
                        future-eligible future-excluded))
           (when (buffer-live-p buffer)
             (kill-buffer buffer))))))))
"####;
    let expected = expect![[
        r#"OK (:enabled (:global t :visible-mode t :visible-rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 5 :characters "~~~~~" :uniform t :faces (page-break-lines)) :visible-window-table-clear t :hidden-mode t :hidden-table nil :excluded-mode nil :excluded-custom-entry t :minibuffer (:minibuffer t :mode nil :table nil :visible-rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 5 :characters "~~~~~" :uniform t :faces (page-break-lines)))) :hidden-when-displayed (:table-subtype display-table :last-extra-slot readable :entry-present t :length 5 :characters "~~~~~" :uniform t :faces (page-break-lines)) :future-buffers (:eligible-mode t :eligible-table nil :excluded-mode nil :excluded-table nil) :disabled (:global nil :local-modes (nil nil nil nil) :eligible-rules ((:table-subtype display-table :last-extra-slot readable :entry-present nil :length nil :characters nil :uniform nil :faces nil) (:table-subtype display-table :last-extra-slot readable :entry-present nil :length nil :characters nil :uniform nil :faces nil) (:table-subtype nil :last-extra-slot nil :entry-present nil :length nil :characters nil :uniform nil :faces nil)) :excluded-custom-entry t))"#
    ]];
    ParityBatchCase::value(
        "global_mode_tracks_eligible_derived_and_new_buffers_but_skips_exclusions",
        elisp_form,
        expected,
    )
}

fn runtime_character_width_cap_and_display_hooks_rebuild_only_when_needed() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-page-break-lines-test-call-with-face-restored
 (lambda ()
   (save-window-excursion
     (let ((buffer (generate-new-buffer " *page-break-lines-live-config*"))
           (page-break-lines-modes
            '(neomacs-page-break-lines-test-base-mode))
           (page-break-lines-char ?=)
           (page-break-lines-max-width 8)
           initial unchanged zero-width changed final hooks-installed
           window original-window-table)
       (unwind-protect
           (progn
             (global-page-break-lines-mode -1)
             (delete-other-windows)
             (setq window (selected-window)
                   original-window-table (window-display-table window)
                   hooks-installed
                   (mapcar
                    (lambda (hook)
                      (and
                       (memq 'page-break-lines--update-display-tables
                             (default-value hook))
                       t))
                    '(window-configuration-change-hook
                      window-size-change-functions
                      after-setting-font-hook
                      display-line-numbers-mode-hook)))
             (set-window-display-table window nil)
             (set-window-buffer window buffer)
             (with-current-buffer buffer
               (neomacs-page-break-lines-test-doc-mode)
               (insert "Runtime configuration\n\f\nRemains navigable\n")
               (page-break-lines-mode 1)
               (let ((entry (neomacs-page-break-lines-test-entry)))
                 (setq initial
                       (list
                        :rule
                        (neomacs-page-break-lines-test-rule-summary)
                        :window-table-clear
                        (null (window-display-table window))))
                 (let ((display-line-numbers-mode-hook
                        '(page-break-lines--update-display-tables)))
                   (display-line-numbers-mode 1))
                 (setq unchanged
                       (list
                        :line-numbers display-line-numbers-mode
                        :same-entry
                        (eq entry (neomacs-page-break-lines-test-entry))
                        :rule
                        (neomacs-page-break-lines-test-rule-summary))))
               (setq-local page-break-lines-max-width 0)
               (let ((display-line-numbers-mode-hook
                      '(page-break-lines--update-display-tables)))
                 (display-line-numbers-mode -1))
               (setq zero-width
                     (list
                      :line-numbers display-line-numbers-mode
                      :rule
                      (neomacs-page-break-lines-test-rule-summary)))
               (setq-local page-break-lines-max-width 5)
               (setq page-break-lines-char ?═)
               (let ((after-setting-font-hook
                      '(page-break-lines--update-display-tables)))
                 (run-hooks 'after-setting-font-hook))
               (setq changed
                     (list
                      :rule
                      (neomacs-page-break-lines-test-rule-summary)
                      :text (buffer-string)
                      :form-feeds
                      (neomacs-page-break-lines-test-form-feed-positions)))
               (page-break-lines-mode -1)
               (setq final
                     (list
                      :mode page-break-lines-mode
                      :line-numbers display-line-numbers-mode
                      :rule
                      (neomacs-page-break-lines-test-rule-summary))))
             (list :hooks-installed hooks-installed
                   :initial initial
                   :unchanged-update unchanged
                   :zero-width zero-width
                   :changed-character changed
                   :final final))
         (global-page-break-lines-mode -1)
         (when (window-live-p window)
           (set-window-display-table window original-window-table))
         (when (buffer-live-p buffer)
           (kill-buffer buffer)))))))
"####;
    let expected = expect![[
        r#"OK (:hooks-installed (t t t t) :initial (:rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 8 :characters "========" :uniform t :faces (page-break-lines)) :window-table-clear t) :unchanged-update (:line-numbers t :same-entry t :rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 8 :characters "========" :uniform t :faces (page-break-lines))) :zero-width (:line-numbers nil :rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 0 :characters "" :uniform nil :faces nil)) :changed-character (:rule (:table-subtype display-table :last-extra-slot readable :entry-present t :length 5 :characters "═════" :uniform t :faces (page-break-lines)) :text "Runtime configuration\n\f\nRemains navigable\n" :form-feeds (23)) :final (:mode nil :line-numbers nil :rule (:table-subtype display-table :last-extra-slot readable :entry-present nil :length nil :characters nil :uniform nil :faces nil)))"#
    ]];
    ParityBatchCase::value(
        "runtime_character_width_cap_and_display_hooks_rebuild_only_when_needed",
        elisp_form,
        expected,
    )
}

#[test]
fn page_break_lines_package_batch() {
    let cases = [
        local_mode_preserves_real_pages_while_installing_a_semantic_rule(),
        display_table_composition_exposes_exact_ownership_on_disable(),
        window_hooks_preserve_unselected_buffer_and_window_points(),
        fractional_pixel_width_and_tiny_windows_produce_exact_rule_lengths(),
        global_mode_tracks_eligible_derived_and_new_buffers_but_skips_exclusions(),
        runtime_character_width_cap_and_display_hooks_rebuild_only_when_needed(),
    ];
    assert_oracle_batch_cases(
        page_break_lines_oracle(),
        "page-break-lines-package-batch",
        "page-break-lines parity",
        &cases,
    );
}
