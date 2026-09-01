//! Practical Corfu parity against the exact locked MELPA source.

use std::time::Duration;

use expect_test::expect;

use crate::{COMPAT_GNU_ELPA_PIN, CORFU_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'corfu)
(require 'corfu-auto)
(require 'corfu-history)
(require 'corfu-indexed)

(defvar-local corfu380-test-events nil)
(defvar-local corfu380-test-observations nil)

(defun corfu380-test-capf ()
  (let ((end (point))
        (start (save-excursion (skip-chars-backward "[:word:]-") (point))))
    (list start end '("café" "camel" "carbide")
          :annotation-function
          (lambda (candidate) (concat "  kind:" (substring candidate 0 2)))
          :exit-function
          (lambda (candidate status)
            (push (list candidate status) corfu380-test-events)))))

(defun corfu380-test-observe-command ()
  (when (or completion-in-region-mode
            (memq this-command '(corfu-insert corfu-quit)))
    (let ((popup (get-buffer " *corfu*")))
      (push
       (list :command this-command
             :active completion-in-region-mode
             :index corfu--index
             :preselect corfu--preselect
             :candidates (mapcar #'substring-no-properties corfu--candidates)
             :preview
             (and (overlayp corfu--preview-ov)
                  (overlay-buffer corfu--preview-ov)
                  (list :start (overlay-start corfu--preview-ov)
                        :end (overlay-end corfu--preview-ov)
                        :display
                        (let ((value (overlay-get corfu--preview-ov 'display)))
                          (and (stringp value)
                               (substring-no-properties value)))
                        :after
                        (let ((value
                               (overlay-get corfu--preview-ov 'after-string)))
                          (and (stringp value)
                               (substring-no-properties value)))
                        :selected-window
                        (eq (overlay-get corfu--preview-ov 'window)
                            (selected-window))))
             :popup (and popup
                         (with-current-buffer popup
                           (buffer-substring-no-properties
                            (point-min) (point-max))))
             :text (buffer-substring-no-properties (point-min) (point-max))
             :point (point))
       corfu380-test-observations))))

(defun corfu380-test-run (thunk)
  (let ((buffers-before (buffer-list))
        (frames-before (frame-list))
        (timers-before (append timer-list timer-idle-list))
        result body-error cleanup-errors)
    (unwind-protect
        (condition-case error
            (setq result
                  (save-window-excursion
                    (save-current-buffer
                      (funcall thunk))))
          (error (setq body-error error)))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case error
              (delete-frame frame t)
            (error (push (list :delete-frame error) cleanup-errors)))))
      (setq corfu--frame nil)
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error
             (push (list :kill-buffer (buffer-name buffer) error)
                   cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error
              (cancel-timer timer)
            (error (push (list :cancel-timer error) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (push (list :remaining-frame frame) cleanup-errors)))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (push (list :remaining-buffer (buffer-name buffer)) cleanup-errors)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (push (list :remaining-timer t) cleanup-errors))))
    (cond
     ((and body-error cleanup-errors)
      (error "Corfu body failed %S; cleanup failed %S"
             body-error (nreverse cleanup-errors)))
     (body-error (signal (car body-error) (cdr body-error)))
     (cleanup-errors
      (error "Corfu cleanup failed: %S" (nreverse cleanup-errors)))
     (t result))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CORFU_MELPA_PIN, "corfu.el")
        .expect("prepare pinned Corfu source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat dependency")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn completion_at_point_navigates_and_inserts_an_annotated_candidate() -> ParityBatchCase {
    ParityBatchCase::value(
        "completion_at_point_navigates_and_inserts_an_annotated_candidate",
        r####"(corfu380-test-run
 (lambda ()
   (let ((buffer (generate-new-buffer " *corfu380-manual*")))
     (switch-to-buffer buffer)
     (let ((completion-at-point-functions '(corfu380-test-capf))
           (corfu-preselect 'prompt)
           (corfu-preview-current nil)
           (corfu-count 3))
       (corfu-mode 1)
       (add-hook 'post-command-hook #'corfu380-test-observe-command 100 t)
       (insert "ca")
       (call-interactively #'completion-at-point)
       (execute-kbd-macro (kbd "<down> RET"))
       (list :mode corfu-mode
             :text (buffer-string)
             :events (nreverse corfu380-test-events)
             :observations (nreverse corfu380-test-observations))))))"####,
        expect![[
            r#"OK (:mode t :text "café" :events (("café" finished)) :observations ((:command nil :active t :index -1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-next :active t :index 0 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-insert :active nil :index -1 :preselect -1 :candidates nil :preview nil :popup nil :text "café" :point 5)))"#
        ]],
    )
}

fn cycling_preview_wraps_through_the_prompt_and_public_cancel_restores_input() -> ParityBatchCase {
    ParityBatchCase::value(
        "cycling_preview_wraps_through_the_prompt_and_public_cancel_restores_input",
        r####"(corfu380-test-run
 (lambda ()
   (let ((buffer (generate-new-buffer " *corfu380-cycle*")))
     (switch-to-buffer buffer)
     (let ((completion-at-point-functions '(corfu380-test-capf))
           (corfu-preselect 'prompt)
           (corfu-preview-current t)
           (corfu-cycle t)
           (corfu-count 3))
       (corfu-mode 1)
       (add-hook 'post-command-hook #'corfu380-test-observe-command 100 t)
       (insert "ca")
       (call-interactively #'completion-at-point)
       (execute-kbd-macro (kbd "<down> <up> <up> C-g"))
       (list :mode corfu-mode
             :active completion-in-region-mode
             :text (buffer-string)
             :events (nreverse corfu380-test-events)
             :preview-live (and (overlayp corfu--preview-ov)
                                (overlay-buffer corfu--preview-ov))
             :observations (nreverse corfu380-test-observations))))))"####,
        expect![[
            r#"OK (:mode t :active nil :text "ca" :events nil :preview-live nil :observations ((:command nil :active t :index -1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-next :active t :index 0 :preselect -1 :candidates ("café" "camel" "carbide") :preview (:start 1 :end 3 :display "café" :after nil :selected-window t) :popup nil :text "ca" :point 3) (:command corfu-previous :active t :index -1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-previous :active t :index 2 :preselect -1 :candidates ("café" "camel" "carbide") :preview (:start 1 :end 3 :display "carbide" :after nil :selected-window t) :popup nil :text "ca" :point 3) (:command corfu-quit :active nil :index -1 :preselect -1 :candidates nil :preview nil :popup nil :text "ca" :point 3)))"#
        ]],
    )
}

fn history_mode_promotes_a_previously_inserted_candidate() -> ParityBatchCase {
    ParityBatchCase::value(
        "history_mode_promotes_a_previously_inserted_candidate",
        r####"(corfu380-test-run
 (lambda ()
   (let ((buffer (generate-new-buffer " *corfu380-history*"))
         (corfu-sort-function corfu-sort-function)
         (corfu-history nil)
         (corfu-history--hash nil)
         first-text first-events first-observations)
     (switch-to-buffer buffer)
     (let ((completion-at-point-functions '(corfu380-test-capf))
           (corfu-preselect 'prompt)
           (corfu-preview-current nil)
           (corfu-count 3))
       (unwind-protect
           (progn
             (corfu-history-mode 1)
             (corfu-mode 1)
             (add-hook 'post-command-hook
                       #'corfu380-test-observe-command 100 t)
             (insert "ca")
             (call-interactively #'completion-at-point)
             (execute-kbd-macro (kbd "<down> <down> <down> RET"))
             (setq first-text (buffer-string)
                   first-events (nreverse corfu380-test-events)
                   first-observations (nreverse corfu380-test-observations)
                   corfu380-test-events nil
                   corfu380-test-observations nil)
             (erase-buffer)
             (insert "ca")
             (call-interactively #'completion-at-point)
             (execute-kbd-macro (kbd "C-g"))
             (list :first
                   (list :text first-text
                         :events first-events
                         :observations first-observations)
                   :history corfu-history
                   :second
                   (list :text (buffer-string)
                         :events (nreverse corfu380-test-events)
                         :observations
                         (nreverse corfu380-test-observations))))
         (corfu-history-mode -1))))))"####,
        expect![[
            r#"OK (:first (:text "carbide" :events (("carbide" finished)) :observations ((:command nil :active t :index -1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-next :active t :index 0 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-next :active t :index 1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-next :active t :index 2 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-insert :active nil :index -1 :preselect -1 :candidates nil :preview nil :popup nil :text "carbide" :point 8))) :history ("carbide") :second (:text "ca" :events nil :observations ((:command nil :active t :index -1 :preselect -1 :candidates ("carbide" "café" "camel") :preview nil :popup nil :text "ca" :point 3) (:command corfu-quit :active nil :index -1 :preselect -1 :candidates nil :preview nil :popup nil :text "ca" :point 3))))"#
        ]],
    )
}

fn indexed_mode_selects_the_requested_candidate_with_a_numeric_prefix() -> ParityBatchCase {
    ParityBatchCase::value(
        "indexed_mode_selects_the_requested_candidate_with_a_numeric_prefix",
        r####"(corfu380-test-run
 (lambda ()
   (let ((buffer (generate-new-buffer " *corfu380-indexed*"))
         (corfu-indexed-start 0))
     (switch-to-buffer buffer)
     (let ((completion-at-point-functions '(corfu380-test-capf))
           (corfu-preselect 'prompt)
           (corfu-preview-current nil)
           (corfu-count 3))
       (unwind-protect
           (progn
             (corfu-indexed-mode 1)
             (corfu-mode 1)
             (add-hook 'post-command-hook
                       #'corfu380-test-observe-command 100 t)
             (insert "ca")
             (call-interactively #'completion-at-point)
             (execute-kbd-macro (kbd "M-2 RET"))
             (list :mode corfu-indexed-mode
                   :text (buffer-string)
                   :events (nreverse corfu380-test-events)
                   :observations
                   (nreverse corfu380-test-observations)))
         (corfu-indexed-mode -1))))))"####,
        expect![[
            r#"OK (:mode t :text "carbide" :events (("carbide" finished)) :observations ((:command nil :active t :index -1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command nil :active t :index -1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-insert :active nil :index -1 :preselect -1 :candidates nil :preview nil :popup nil :text "carbide" :point 8)))"#
        ]],
    )
}

fn automatic_completion_activates_from_typing_and_inserts_a_candidate() -> ParityBatchCase {
    ParityBatchCase::value(
        "automatic_completion_activates_from_typing_and_inserts_a_candidate",
        r####"(corfu380-test-run
 (lambda ()
   (let ((buffer (generate-new-buffer " *corfu380-auto*")))
     (switch-to-buffer buffer)
     (let ((completion-at-point-functions '(corfu380-test-capf))
           (corfu-auto t)
           (corfu-auto-prefix 2)
           (corfu-auto-delay 0)
           (corfu-preselect 'prompt)
           (corfu-preview-current nil)
           (corfu-count 3))
       (corfu-mode 1)
       (add-hook 'post-command-hook
                 #'corfu380-test-observe-command 100 t)
       (execute-kbd-macro (kbd "c a"))
       (execute-kbd-macro (kbd "<down> RET"))
       (list :mode corfu-mode
             :text (buffer-string)
             :events (nreverse corfu380-test-events)
             :observations
             (nreverse corfu380-test-observations))))))"####,
        expect![[
            r#"OK (:mode t :text "café" :events (("café" finished)) :observations ((:command self-insert-command :active t :index -1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command nil :active t :index -1 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-next :active t :index 0 :preselect -1 :candidates ("café" "camel" "carbide") :preview nil :popup nil :text "ca" :point 3) (:command corfu-insert :active nil :index -1 :preselect -1 :candidates nil :preview nil :popup nil :text "café" :point 5)))"#
        ]],
    )
}

#[test]
fn corfu_package_batch() {
    let cases = vec![
        completion_at_point_navigates_and_inserts_an_annotated_candidate(),
        cycling_preview_wraps_through_the_prompt_and_public_cancel_restores_input(),
        history_mode_promotes_a_previously_inserted_candidate(),
        indexed_mode_selects_the_requested_candidate_with_a_numeric_prefix(),
        automatic_completion_activates_from_typing_and_inserts_a_candidate(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Corfu parity test");
    assert_oracle_batch_cases(oracle(), test_name, "corfu_parity", &cases);
}
