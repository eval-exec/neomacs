use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVAL_SEXP_FU_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'eval-sexp-fu)

(eval-sexp-fu-flash-mode -1)

(defun neomacs-esf-test-overlay-summary (begin end)
  (mapcar
   (lambda (overlay)
     (list :start (overlay-start overlay)
           :end (overlay-end overlay)
           :face (overlay-get overlay 'face)
           :eval-sexp-fu (overlay-get overlay 'esf-highlight)
           :priority (overlay-get overlay 'priority)))
   (sort (copy-sequence (overlays-in begin end))
         (lambda (left right)
           (or (< (overlay-start left) (overlay-start right))
               (and (= (overlay-start left) (overlay-start right))
                    (< (overlay-end left) (overlay-end right))))))))

(defun neomacs-esf-test-capture (function)
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-esf-test-transaction (strategy fail)
  (let ((eval-sexp-fu-flash-doit-function strategy)
        events scheduled outcome before-cleanup)
    (cl-letf (((symbol-function 'run-at-time)
               (lambda (seconds repeat function &rest arguments)
                 (setq scheduled (cons function arguments))
                 (push (list :scheduled seconds repeat) events)
                 'neomacs-esf-test-timer)))
      (setq outcome
            (neomacs-esf-test-capture
             (lambda ()
               (esf-flash-doit
                (lambda ()
                  (push :evaluate events)
                  (if fail (error "deployment rejected") :published))
                (lambda () (push :highlight events))
                (lambda () (push :unhighlight events))
                (lambda () (push :error-flash events))))))
      (setq before-cleanup (copy-tree (reverse events)))
      (when scheduled (apply (car scheduled) (cdr scheduled)))
      (list :outcome outcome
            :before-cleanup before-cleanup
            :after-cleanup (reverse events)))))

(defvar neomacs-esf-test-advice-result nil)

(defun neomacs-esf-test-advised-evaluation (form flash-mode)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert form)
    (goto-char (point-max))
    (setq neomacs-esf-test-advice-result nil)
    (let* ((eval-sexp-fu-flash-mode flash-mode)
           (eval-expression-debug-on-error nil)
           flashed-bounds events scheduled outcome before-cleanup
           (eval-sexp-fu-flash-function
            (lambda (bounds _face _error-face _buffer)
              (setq flashed-bounds bounds)
              (values bounds
                      (lambda () (push :highlight events))
                      (lambda () (push :unhighlight events))
                      (lambda () (push :error-flash events))))))
      (cl-letf
          (((symbol-function 'run-at-time)
            (lambda (seconds repeat function &rest arguments)
              (setq scheduled (cons function arguments))
              (push (list :scheduled seconds repeat) events)
              'neomacs-esf-test-timer)))
        (setq outcome
              (neomacs-esf-test-capture
               (lambda () (eval-last-sexp nil))))
        (setq before-cleanup (copy-tree (reverse events)))
        (when scheduled (apply (car scheduled) (cdr scheduled)))
        (list :outcome outcome
              :result neomacs-esf-test-advice-result
              :bounds flashed-bounds
              :before-cleanup before-cleanup
              :after-cleanup (reverse events))))))
"####;

fn inner_commands_evaluate_the_expression_and_enclosing_business_form_at_point() -> ParityBatchCase
{
    let elisp_form = r####"
(progn
  (defvar neomacs-esf-test-line-total nil)
  (defvar neomacs-esf-test-invoice nil)
  (setq neomacs-esf-test-line-total nil
        neomacs-esf-test-invoice nil)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(progn\n"
            "  (setq neomacs-esf-test-line-total (+ 19 23))\n"
            "  (setq neomacs-esf-test-invoice\n"
            "        (list :line neomacs-esf-test-line-total :tax 8)))")
    (goto-char (point-min))
    (search-forward "(setq neomacs-esf-test-line-total")
    (goto-char (match-beginning 0))
    (eval-sexp-fu-eval-sexp-inner-sexp)
    (let ((line-result neomacs-esf-test-line-total))
      (goto-char (point-min))
      (search-forward ":tax")
      (eval-sexp-fu-eval-sexp-inner-list 2)
      (list :line-result line-result
            :invoice neomacs-esf-test-invoice
            :point (point)
            :buffer-size (buffer-size)))))
"####;
    let expected =
        expect!["OK (:line-result 42 :invoice (:line 42 :tax 8) :point 140 :buffer-size 144)"];
    ParityBatchCase::value(
        "inner_commands_evaluate_the_expression_and_enclosing_business_form_at_point",
        elisp_form,
        expected,
    )
}

fn blank_line_navigation_selects_the_nearest_pipeline_stage_for_evaluation() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(setq deployment-stage 'build)\n\n\n\n(setq deployment-stage 'release)")
  (cl-labels ((selected-at-line
               (line)
               (goto-char (point-min))
               (forward-line line)
               (esf-forward-inner-sexp)
               (elisp--preceding-sexp)))
    (list :after-build (selected-at-line 1)
          :equidistant (selected-at-line 2)
          :before-release (selected-at-line 3)
          :on-release
          (progn
            (goto-char (point-min))
            (search-forward "(setq deployment-stage 'release)")
            (goto-char (match-beginning 0))
            (esf-forward-inner-sexp)
            (elisp--preceding-sexp)))))
"####;
    let expected = expect![
        "OK (:after-build (setq deployment-stage 'build) :equidistant (setq deployment-stage 'build) :before-release (setq deployment-stage 'release) :on-release (setq deployment-stage 'release))"
    ];
    ParityBatchCase::value(
        "blank_line_navigation_selects_the_nearest_pipeline_stage_for_evaluation",
        elisp_form,
        expected,
    )
}

fn full_expression_flash_preserves_foreign_overlays_and_cleans_up_its_own() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(deploy-release :channel 'stable)")
  (let* ((bounds (cons (point-min) (point-max)))
         (foreign (make-overlay 2 8)))
    (overlay-put foreign 'face 'audit-marker)
    (cl-multiple-value-bind (actual highlight unhighlight error-flash)
        (eval-sexp-fu-flash-default
         bounds 'eval-success 'eval-failure (current-buffer))
      (funcall highlight)
      (let ((during (neomacs-esf-test-overlay-summary (point-min) (point-max))))
        (funcall unhighlight)
        (list :bounds actual
              :during during
              :after (neomacs-esf-test-overlay-summary (point-min) (point-max))
              :foreign-live (eq (overlay-buffer foreign) (current-buffer))
              :error-callback (functionp error-flash))))))
"####;
    let expected = expect![
        "OK (:bounds (1 . 34) :during ((:start 1 :end 34 :face eval-success :eval-sexp-fu t :priority 0) (:start 2 :end 8 :face audit-marker :eval-sexp-fu nil :priority nil)) :after ((:start 2 :end 8 :face audit-marker :eval-sexp-fu nil :priority nil)) :foreign-live t :error-callback t)"
    ];
    ParityBatchCase::value(
        "full_expression_flash_preserves_foreign_overlays_and_cleans_up_its_own",
        elisp_form,
        expected,
    )
}

fn delimiter_flash_and_error_feedback_follow_the_visible_overlay_lifecycle() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(publish-release :channel 'stable)")
  (let* ((bounds (cons (point-min) (point-max)))
         idle-calls cleanup-calls)
    (cl-letf (((symbol-function 'run-with-idle-timer)
               (lambda (seconds repeat function &rest arguments)
                 (push (list seconds repeat function arguments) idle-calls)
                 'neomacs-esf-test-idle-timer))
              ((symbol-function 'run-at-time)
               (lambda (seconds repeat function &rest arguments)
                 (push (list seconds repeat function arguments) cleanup-calls)
                 'neomacs-esf-test-timer)))
      (cl-multiple-value-bind (_actual highlight unhighlight error-flash)
          (esf-flash-paren-only
           bounds 'eval-success 'eval-failure (current-buffer))
        (funcall highlight)
        (let ((success (neomacs-esf-test-overlay-summary (point-min) (point-max))))
          (funcall unhighlight)
          (funcall error-flash)
          (dolist (call (reverse idle-calls))
            (apply (nth 2 call) (nth 3 call)))
          (let ((failure (neomacs-esf-test-overlay-summary
                          (point-min) (point-max))))
            (dolist (call (reverse cleanup-calls))
              (apply (nth 2 call) (nth 3 call)))
            (list :success success
                  :after-success (null (overlays-in (point-min) (point-max)))
                  :idle-delays (mapcar #'car (reverse idle-calls))
                  :failure failure
                  :cleanup-delays (mapcar #'car (reverse cleanup-calls))
                  :after-failure (overlays-in (point-min) (point-max)))))))))
"####;
    let expected = expect![
        "OK (:success ((:start 1 :end 2 :face eval-success :eval-sexp-fu t :priority 0) (:start 34 :end 35 :face eval-success :eval-sexp-fu t :priority 0)) :after-success t :idle-delays (0.3 0.3) :failure ((:start 1 :end 2 :face eval-failure :eval-sexp-fu t :priority 0) (:start 34 :end 35 :face eval-failure :eval-sexp-fu t :priority 0)) :cleanup-delays (0.3 0.3) :after-failure nil)"
    ];
    ParityBatchCase::value(
        "delimiter_flash_and_error_feedback_follow_the_visible_overlay_lifecycle",
        elisp_form,
        expected,
    )
}

fn flash_transactions_order_success_cleanup_and_error_feedback() -> ParityBatchCase {
    let elisp_form = r####"
(list :simple-success
      (neomacs-esf-test-transaction 'eval-sexp-fu-flash-doit-simple nil)
      :simple-failure
      (neomacs-esf-test-transaction 'eval-sexp-fu-flash-doit-simple t)
      :hold-failure
      (neomacs-esf-test-transaction 'eval-sexp-fu-flash-doit-hold-on-error t))
"####;
    let expected = expect![[
        r#"OK (:simple-success (:outcome (:ok :published) :before-cleanup (:highlight (:scheduled 0.15 nil) :evaluate) :after-cleanup (:highlight (:scheduled 0.15 nil) :evaluate :unhighlight)) :simple-failure (:outcome (:error error :data ("deployment rejected") :message "deployment rejected") :before-cleanup (:highlight (:scheduled 0.15 nil) :evaluate :error-flash) :after-cleanup (:highlight (:scheduled 0.15 nil) :evaluate :error-flash :unhighlight)) :hold-failure (:outcome (:error error :data ("deployment rejected") :message "deployment rejected") :before-cleanup (:highlight :evaluate (:scheduled 0.15 nil) :error-flash) :after-cleanup (:highlight :evaluate (:scheduled 0.15 nil) :error-flash :unhighlight)))"#
    ]];
    ParityBatchCase::value(
        "flash_transactions_order_success_cleanup_and_error_feedback",
        elisp_form,
        expected,
    )
}

fn eval_last_sexp_advice_is_mode_gated_and_flashes_successes_and_failures() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :disabled
 (neomacs-esf-test-advised-evaluation
  "(setq neomacs-esf-test-advice-result (+ 20 22))" nil)
 :enabled
 (neomacs-esf-test-advised-evaluation
  "(setq neomacs-esf-test-advice-result (+ 20 22))" t)
 :failure
 (neomacs-esf-test-advised-evaluation
  "(error \"deployment rejected\")" t))
"####;
    let expected = expect![[
        r#"OK (:disabled (:outcome (:ok 42) :result 42 :bounds nil :before-cleanup nil :after-cleanup nil) :enabled (:outcome (:ok 42) :result 42 :bounds (1 . 48) :before-cleanup (:highlight (:scheduled 0.15 nil)) :after-cleanup (:highlight (:scheduled 0.15 nil) :unhighlight)) :failure (:outcome (:error error :data ("deployment rejected") :message "deployment rejected") :result nil :bounds (1 . 30) :before-cleanup (:highlight (:scheduled 0.15 nil) :error-flash) :after-cleanup (:highlight (:scheduled 0.15 nil) :error-flash :unhighlight)))"#
    ]];
    ParityBatchCase::value(
        "eval_last_sexp_advice_is_mode_gated_and_flashes_successes_and_failures",
        elisp_form,
        expected,
    )
}

#[test]
fn eval_sexp_fu_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(EVAL_SEXP_FU_MELPA_PIN, "eval-sexp-fu.el")
            .expect("prepare revision-pinned Eval Sexp Fu source below ./tmp")
            .with_timeout(Duration::from_secs(120))
            .with_prelude(PRELUDE),
        "eval-sexp-fu-package-batch",
        "Eval Sexp Fu",
        &[
            inner_commands_evaluate_the_expression_and_enclosing_business_form_at_point(),
            blank_line_navigation_selects_the_nearest_pipeline_stage_for_evaluation(),
            full_expression_flash_preserves_foreign_overlays_and_cleans_up_its_own(),
            delimiter_flash_and_error_feedback_follow_the_visible_overlay_lifecycle(),
            flash_transactions_order_success_cleanup_and_error_feedback(),
            eval_last_sexp_advice_is_mode_gated_and_flashes_successes_and_failures(),
        ],
    );
}
