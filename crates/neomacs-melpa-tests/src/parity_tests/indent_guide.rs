use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, INDENT_GUIDE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const INDENT_GUIDE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const INDENT_GUIDE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'indent-guide)

(defun neomacs-indent-guide-test-in-buffer (text function)
  "Display TEXT in a work buffer and call FUNCTION there."
  (let ((buffer (generate-new-buffer "*indent-guide-parity*")))
    (unwind-protect
        (save-window-excursion
          (delete-other-windows)
          (switch-to-buffer buffer)
          (insert text)
          (goto-char (point-min))
          (set-window-start (selected-window) (point-min))
          (funcall function))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(defun neomacs-indent-guide-test-goto-line (line)
  "Move to the indentation of one-based LINE."
  (goto-char (point-min))
  (forward-line (1- line))
  (back-to-indentation))

(defun neomacs-indent-guide-test-styled-string (string)
  "Return stable visible text and per-character faces for STRING."
  (when string
    (list :text (substring-no-properties string)
          :faces
          (let ((index 0)
                faces)
            (while (< index (length string))
              (push (get-text-property index 'face string) faces)
              (setq index (1+ index)))
            (nreverse faces)))))

(defun neomacs-indent-guide-test-overlays ()
  "Return ordered, complete summaries of rendered indent guides."
  (mapcar
   (lambda (overlay)
     (list :range (list (overlay-start overlay) (overlay-end overlay))
           :line (line-number-at-pos (overlay-start overlay))
           :column
           (save-excursion
             (goto-char (overlay-start overlay))
             (current-column))
           :before
           (neomacs-indent-guide-test-styled-string
            (overlay-get overlay 'before-string))
           :display
           (neomacs-indent-guide-test-styled-string
            (overlay-get overlay 'display))))
   (sort
    (cl-remove-if-not
     (lambda (overlay)
       (eq (overlay-get overlay 'category) 'indent-guide))
     (overlays-in (point-min) (point-max)))
    (lambda (left right)
      (if (= (overlay-start left) (overlay-start right))
          (< (overlay-end left) (overlay-end right))
        (< (overlay-start left) (overlay-start right)))))))

(defun neomacs-indent-guide-test-hook-count (hook function)
  "Count local registrations of FUNCTION in HOOK."
  (if (local-variable-p hook)
      (cl-count function (symbol-value hook))
    0))
"####;

fn indent_guide_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(INDENT_GUIDE_MELPA_PIN, "indent-guide.el")
        .expect("prepare revision-pinned Indent Guide source below ./tmp")
        .with_prelude(INDENT_GUIDE_TEST_PRELUDE)
        .with_timeout(INDENT_GUIDE_TEST_TIMEOUT)
}

fn decorated_space_guides_span_a_real_nested_block() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-indent-guide-test-in-buffer
 "root\n  child\n    alpha\n    beta\n\n    gamma\n  sibling\ntail\n"
 (lambda ()
   (setq-local indent-guide-char "│"
               indent-guide-char-top "╭"
               indent-guide-char-bottom "╰"
               indent-guide-recursive nil
               indent-guide-threshold -1)
   (neomacs-indent-guide-test-goto-line 4)
   (indent-guide-show)
   (list :point (list (line-number-at-pos) (current-column))
         :text (buffer-string)
         :guides (neomacs-indent-guide-test-overlays))))
"####;
    let expected = expect![[
        r#"OK (:point (4 4) :text "root\n  child\n    alpha\n    beta\n\n    gamma\n  sibling\ntail\n" :guides ((:range (16 17) :line 3 :column 2 :before nil :display (:text "╭" :faces (indent-guide-face))) (:range (26 27) :line 4 :column 2 :before nil :display (:text "│" :faces (indent-guide-face))) (:range (33 33) :line 5 :column 0 :before (:text "  │" :faces (nil nil indent-guide-face)) :display nil) (:range (36 37) :line 6 :column 2 :before nil :display (:text "╰" :faces (indent-guide-face)))))"#
    ]];
    ParityBatchCase::value(
        "decorated_space_guides_span_a_real_nested_block",
        elisp_form,
        expected,
    )
}

fn mixed_tab_and_blank_lines_render_at_the_visual_parent_column() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-indent-guide-test-in-buffer
 "root\n  branch\n\tleaf\n\n\tpeer\n  tail\n"
 (lambda ()
   (setq-local tab-width 8
               indent-tabs-mode t
               indent-guide-char "│"
               indent-guide-char-top "╭"
               indent-guide-char-bottom "╰"
               indent-guide-recursive nil
               indent-guide-threshold -1)
   (neomacs-indent-guide-test-goto-line 3)
   (indent-guide-show)
   (list :point (list (line-number-at-pos) (current-column))
         :tab-width tab-width
         :guides (neomacs-indent-guide-test-overlays))))
"####;
    let expected = expect![[
        r#"OK (:point (3 8) :tab-width 8 :guides ((:range (15 16) :line 3 :column 0 :before nil :display (:text "  ╭     " :faces (nil nil indent-guide-face nil nil nil nil nil))) (:range (21 21) :line 4 :column 0 :before (:text "  │" :faces (nil nil indent-guide-face)) :display nil) (:range (22 23) :line 5 :column 0 :before nil :display (:text "  ╰     " :faces (nil nil indent-guide-face nil nil nil nil nil)))))"#
    ]];
    ParityBatchCase::value(
        "mixed_tab_and_blank_lines_render_at_the_visual_parent_column",
        elisp_form,
        expected,
    )
}

fn recursive_guides_render_each_blank_line_column_and_respect_thresholds() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-indent-guide-test-in-buffer
 "root\n  project\n    task\n      build\n\n      deploy\n    peer\n  tail\n"
 (lambda ()
   (setq-local indent-guide-char "|"
               indent-guide-char-top nil
               indent-guide-char-bottom nil
               indent-guide-recursive t
               indent-guide-threshold 0)
   (neomacs-indent-guide-test-goto-line 6)
   (indent-guide-show)
   (let ((threshold-zero (neomacs-indent-guide-test-overlays)))
     (setq-local indent-guide-threshold 2)
     (indent-guide-show)
     (list :threshold-zero threshold-zero
           :threshold-two (neomacs-indent-guide-test-overlays)))))
"####;
    let expected = expect![[
        r#"OK (:threshold-zero ((:range (18 19) :line 3 :column 2 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (27 28) :line 4 :column 2 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (29 30) :line 4 :column 4 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (37 37) :line 5 :column 0 :before (:text "    |" :faces (nil nil nil nil indent-guide-face)) :display nil) (:range (37 37) :line 5 :column 0 :before (:text "  |" :faces (nil nil indent-guide-face)) :display nil) (:range (40 41) :line 6 :column 2 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (42 43) :line 6 :column 4 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (53 54) :line 7 :column 2 :before nil :display (:text "|" :faces (indent-guide-face)))) :threshold-two ((:range (29 30) :line 4 :column 4 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (37 37) :line 5 :column 0 :before (:text "    |" :faces (nil nil nil nil indent-guide-face)) :display nil) (:range (42 43) :line 6 :column 4 :before nil :display (:text "|" :faces (indent-guide-face)))))"#
    ]];
    ParityBatchCase::value(
        "recursive_guides_render_each_blank_line_column_and_respect_thresholds",
        elisp_form,
        expected,
    )
}

fn live_editing_and_navigation_refresh_guides_through_mode_hooks() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-indent-guide-test-in-buffer
 "pipeline\n  build\n    compile\n    test\n  deploy\n"
 (lambda ()
   (setq-local indent-guide-delay nil
               indent-guide-char "|"
               indent-guide-char-top nil
               indent-guide-char-bottom nil
               indent-guide-recursive nil
               indent-guide-threshold -1)
   (let ((foreign (make-overlay (point-min) (1+ (point-min))))
         enabled edited navigated disabled cleaned)
     (overlay-put foreign 'audit-overlay 'keep)
     (neomacs-indent-guide-test-goto-line 3)
     (indent-guide-mode 1)
     (run-hooks 'post-command-hook)
     (setq enabled
           (list :mode indent-guide-mode
                 :lighter (assq 'indent-guide-mode minor-mode-alist)
                 :post-command-count
                 (neomacs-indent-guide-test-hook-count
                  'post-command-hook 'indent-guide--request-show)
                 :scroll-count
                 (neomacs-indent-guide-test-hook-count
                  'window-scroll-functions 'indent-guide--request-show)
                 :guides (neomacs-indent-guide-test-overlays)))
     (save-excursion
       (goto-char (point-min))
       (forward-line 3)
       (delete-char 2))
     (neomacs-indent-guide-test-goto-line 3)
     (run-hooks 'post-command-hook)
     (setq edited
           (list :text (buffer-string)
                 :guides (neomacs-indent-guide-test-overlays)))
     (neomacs-indent-guide-test-goto-line 5)
     (run-hooks 'post-command-hook)
     (setq navigated
           (list :point (list (line-number-at-pos) (current-column))
                 :guides (neomacs-indent-guide-test-overlays)))
     (indent-guide-mode -1)
     (setq disabled
           (list :mode indent-guide-mode
                 :post-command-count
                 (neomacs-indent-guide-test-hook-count
                  'post-command-hook 'indent-guide--request-show)
                 :scroll-count
                 (neomacs-indent-guide-test-hook-count
                  'window-scroll-functions 'indent-guide--request-show)
                 :guides (neomacs-indent-guide-test-overlays)))
     (indent-guide-remove)
     (setq cleaned
           (list :guides (neomacs-indent-guide-test-overlays)
                 :foreign (list (overlay-start foreign)
                                (overlay-end foreign)
                                (overlay-get foreign 'audit-overlay))))
     (list :enabled enabled
           :edited edited
           :navigated navigated
           :disabled disabled
           :cleaned cleaned))))
"####;
    let expected = expect![[
        r#"OK (:enabled (:mode t :lighter (indent-guide-mode " ing") :post-command-count 1 :scroll-count 1 :guides ((:range (20 21) :line 3 :column 2 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (32 33) :line 4 :column 2 :before nil :display (:text "|" :faces (indent-guide-face))))) :edited (:text "pipeline\n  build\n    compile\n  test\n  deploy\n" :guides ((:range (20 21) :line 3 :column 2 :before nil :display (:text "|" :faces (indent-guide-face))))) :navigated (:point (5 2) :guides ((:range (10 11) :line 2 :column 0 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (18 19) :line 3 :column 0 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (30 31) :line 4 :column 0 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (37 38) :line 5 :column 0 :before nil :display (:text "|" :faces (indent-guide-face))))) :disabled (:mode nil :post-command-count 0 :scroll-count 0 :guides ((:range (10 11) :line 2 :column 0 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (18 19) :line 3 :column 0 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (30 31) :line 4 :column 0 :before nil :display (:text "|" :faces (indent-guide-face))) (:range (37 38) :line 5 :column 0 :before nil :display (:text "|" :faces (indent-guide-face))))) :cleaned (:guides nil :foreign (1 2 keep)))"#
    ]];
    ParityBatchCase::value(
        "live_editing_and_navigation_refresh_guides_through_mode_hooks",
        elisp_form,
        expected,
    )
}

fn delayed_refresh_requests_debounce_to_the_last_idle_timer() -> ParityBatchCase {
    let elisp_form = r####"
(let ((indent-guide-delay 0.25)
      (indent-guide--timer-object nil)
      (next-id 0)
      scheduled cancelled shown)
  (cl-letf (((symbol-function 'run-with-idle-timer)
             (lambda (seconds repeat function &rest arguments)
               (setq next-id (1+ next-id))
               (let ((timer (intern (format "indent-timer-%d" next-id))))
                 (push (list timer seconds repeat function arguments) scheduled)
                 timer)))
            ((symbol-function 'cancel-timer)
             (lambda (timer) (push timer cancelled)))
            ((symbol-function 'indent-guide-show)
             (lambda () (push 'rendered shown))))
    (indent-guide--request-show 'first-window 10)
    (indent-guide--request-show 'second-window 20)
    (indent-guide--request-show 'third-window 30)
    (let ((last-timer indent-guide--timer-object))
      (indent-guide--run-timer)
      (list :scheduled (nreverse scheduled)
            :cancelled (nreverse cancelled)
            :last-timer last-timer
            :shown (nreverse shown)
            :timer-after-render indent-guide--timer-object))))
"####;
    let expected = expect![
        "OK (:scheduled ((indent-timer-1 0.25 nil indent-guide--run-timer nil) (indent-timer-2 0.25 nil indent-guide--run-timer nil) (indent-timer-3 0.25 nil indent-guide--run-timer nil)) :cancelled (indent-timer-1 indent-timer-2) :last-timer indent-timer-3 :shown (rendered) :timer-after-render nil)"
    ];
    ParityBatchCase::value(
        "delayed_refresh_requests_debounce_to_the_last_idle_timer",
        elisp_form,
        expected,
    )
}

#[test]
fn indent_guide_package_batch() {
    assert_oracle_batch_cases(
        indent_guide_oracle(),
        "indent-guide-package-batch",
        "Indent Guide",
        &[
            decorated_space_guides_span_a_real_nested_block(),
            mixed_tab_and_blank_lines_render_at_the_visual_parent_column(),
            recursive_guides_render_each_blank_line_column_and_respect_thresholds(),
            live_editing_and_navigation_refresh_guides_through_mode_hooks(),
            delayed_refresh_requests_debounce_to_the_last_idle_timer(),
        ],
    );
}
