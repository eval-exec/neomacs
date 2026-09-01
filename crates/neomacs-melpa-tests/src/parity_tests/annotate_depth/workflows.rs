use expect_test::expect;

use super::ParityBatchCase;

fn readme_prog_mode_setup_marks_excessive_nesting_in_real_lisp_and_cpp_buffers() -> ParityBatchCase
{
    ParityBatchCase::value(
        "readme_prog_mode_setup_marks_excessive_nesting_in_real_lisp_and_cpp_buffers",
        r##"
(let ((scheduled nil)
      (cancelled nil)
      (timer-sequence 0)
      (annotate-depth-idle-timeout 2)
      results)
  (cl-labels
      ((highlighted-lines ()
         (mapcar
          (lambda (overlay)
            (list
             (line-number-at-pos (overlay-start overlay))
             (save-excursion
               (goto-char (overlay-start overlay))
               (current-indentation))
             (buffer-substring-no-properties
              (overlay-start overlay) (overlay-end overlay))
             (overlay-get overlay 'face)))
          (sort
           (copy-sequence annotate-depth--overlays)
           (lambda (a b) (< (overlay-start a) (overlay-start b))))))
       (exercise-buffer (name mode source)
         (let ((buffer (generate-new-buffer name))
               callback
               timer)
           (unwind-protect
               (with-current-buffer buffer
                 (insert source)
                 (funcall mode)
                 ;; The README's threshold customization runs from the minor
                 ;; mode hook, after the initial scan.  The scheduled idle
                 ;; callback is what applies that user setting in practice.
                 (setq timer annotate-depth--idle-timer
                       callback (nth 2 (car scheduled)))
                 (funcall callback)
                 (let ((snapshot
                        (list
                         major-mode
                         annotate-depth-mode
                         annotate-depth-threshold
                         (highlighted-lines)
                         (buffer-substring-no-properties
                          (point-min) (point-max))
                         timer)))
                   (annotate-depth-mode -1)
                   (append
                    snapshot
                    (list
                     annotate-depth-mode
                     annotate-depth--overlays))))
             (when (buffer-live-p buffer)
               (kill-buffer buffer))))))
    (let ((prog-mode-hook
           (cons #'annotate-depth-mode prog-mode-hook))
          (annotate-depth-mode-hook
           (list
            (lambda ()
              (cond
               ((eq major-mode 'emacs-lisp-mode)
                (setq-local annotate-depth-threshold 3))
               ((eq major-mode 'c++-mode)
                (setq-local annotate-depth-threshold 2)))))))
      (cl-letf (((symbol-function 'run-with-idle-timer)
                 (lambda (seconds repeat callback)
                   (setq timer-sequence (1+ timer-sequence))
                   (push (list seconds repeat callback) scheduled)
                   (intern (format "review-timer-%d" timer-sequence))))
                ((symbol-function 'cancel-timer)
                 (lambda (timer)
                   (push timer cancelled))))
        (push
         (exercise-buffer
          " *annotate-depth-elisp*"
          #'emacs-lisp-mode
          (concat
           "(defun publish-order (order)\n"
           "  (when order\n"
           "    (let ((receipt (charge order)))\n"
           "      (when receipt\n"
           "        (message \"sent: %s\" receipt)))))\n"))
         results)
        (push
         (exercise-buffer
          " *annotate-depth-cpp*"
          #'c++-mode
          (concat
           "int checkout(Order order) {\n"
           "  if (order.valid()) {\n"
           "    for (auto item : order.items()) {\n"
           "      if (item.in_stock()) {\n"
           "        charge(item);\n"
           "      }\n"
           "    }\n"
           "  }\n"
           "}\n"))
         results)))
    (list
     (nreverse results)
     (nreverse scheduled)
     (nreverse cancelled))))
"##,
        expect![[
            r#"OK (((emacs-lisp-mode t 3 ((4 6 "(when receipt" annotate-depth) (5 8 "(message \"sent: %s\" receipt)))))" annotate-depth)) "(defun publish-order (order)\n  (when order\n    (let ((receipt (charge order)))\n      (when receipt\n        (message \"sent: %s\" receipt)))))\n" review-timer-1 nil nil) (c++-mode t 2 ((3 4 "for (auto item : order.items()) {" annotate-depth) (4 6 "if (item.in_stock()) {" annotate-depth) (5 8 "charge(item);" annotate-depth) (6 6 "}" annotate-depth) (7 4 "}" annotate-depth)) "int checkout(Order order) {\n  if (order.valid()) {\n    for (auto item : order.items()) {\n      if (item.in_stock()) {\n        charge(item);\n      }\n    }\n  }\n}\n" review-timer-2 nil nil)) ((2 t annotate-depth--annotate) (2 t annotate-depth--annotate)) (review-timer-1 review-timer-2))"#
        ]],
    )
}

fn idle_rescan_replaces_stale_highlights_after_a_real_refactoring_and_disable_cleans_up()
-> ParityBatchCase {
    ParityBatchCase::value(
        "idle_rescan_replaces_stale_highlights_after_a_real_refactoring_and_disable_cleans_up",
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert
   "(defun checkout (order)\n"
   "  (when order\n"
   "    (let ((cents (order-total order)))\n"
   "      (charge cents))))\n")
  (let ((annotate-depth-threshold 2)
        (annotate-depth-idle-timeout 1.5)
        scheduled
        cancelled)
    (cl-labels
        ((highlighted-lines ()
           (mapcar
            (lambda (overlay)
              (list
               (line-number-at-pos (overlay-start overlay))
               (buffer-substring-no-properties
                (overlay-start overlay) (overlay-end overlay))))
            (sort
             (copy-sequence annotate-depth--overlays)
             (lambda (a b) (< (overlay-start a) (overlay-start b)))))))
      (cl-letf (((symbol-function 'run-with-idle-timer)
                 (lambda (seconds repeat callback)
                   (setq scheduled (list seconds repeat callback))
                   'refactor-idle-timer))
                ((symbol-function 'cancel-timer)
                 (lambda (timer)
                   (setq cancelled timer))))
        (annotate-depth-mode 1)
        (let ((before
               (list
                (highlighted-lines)
                (buffer-substring-no-properties
                 (point-min) (point-max)))))
          ;; Replace the nested temporary-binding block with the shallower
          ;; expression a reviewer would actually prefer.
          (goto-char (point-min))
          (forward-line 2)
          (delete-region (line-beginning-position) (point-max))
          (insert "    (charge (order-total order))))\n")
          (let ((before-idle (highlighted-lines)))
            (funcall (nth 2 scheduled))
            (let ((after-idle
                   (list
                    (highlighted-lines)
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))
              (annotate-depth-mode -1)
              (list
               before
               before-idle
               after-idle
               scheduled
               cancelled
               annotate-depth-mode
               annotate-depth--overlays
               (buffer-substring-no-properties
                (point-min) (point-max))))))))))
"##,
        expect![[
            r#"OK ((((3 "(let ((cents (order-total order)))") (4 "(charge cents))))")) "(defun checkout (order)\n  (when order\n    (let ((cents (order-total order)))\n      (charge cents))))\n") ((4 "") (4 "")) (((3 "(charge (order-total order))))")) "(defun checkout (order)\n  (when order\n    (charge (order-total order))))\n") (1.5 t annotate-depth--annotate) refactor-idle-timer nil nil "(defun checkout (order)\n  (when order\n    (charge (order-total order))))\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        readme_prog_mode_setup_marks_excessive_nesting_in_real_lisp_and_cpp_buffers(),
        idle_rescan_replaces_stale_highlights_after_a_real_refactoring_and_disable_cleans_up(),
    ]
}
