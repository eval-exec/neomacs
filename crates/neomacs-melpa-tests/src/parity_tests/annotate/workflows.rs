use expect_test::expect;

use super::ParityBatchCase;

fn review_notes_survive_reopen_and_follow_their_code_after_an_external_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "review_notes_survive_reopen_and_follow_their_code_after_an_external_edit",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "annotate-persistent-review"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source-file (expand-file-name "invoice.el" root))
       (annotate-file (expand-file-name "review-notes.el" root))
       (annotate-use-echo-area t)
       (annotate-use-messages nil)
       (annotate-warn-if-hash-mismatch nil)
       (annotate-database-confirm-deletion nil)
       (annotate-autosave t)
       first-session
       reopened-session
       source-buffer)
  (unwind-protect
      (progn
        (make-directory root t)
        (with-temp-file source-file
          (insert
           "(defun invoice-total (items)\n"
           "  (let ((subtotal 0))\n"
           "    (dolist (item items subtotal)\n"
           "      (setq subtotal (+ subtotal item)))))\n"))

        ;; Follow the README quick start: annotate one selected expression and
        ;; one symbol through the interactive user command.
        (setq source-buffer (find-file-noselect source-file))
        (with-current-buffer source-buffer
          (emacs-lisp-mode)
          (transient-mark-mode 1)
          (annotate-mode 1)
          (let ((answers
                 '("This accumulator update needs an overflow policy."
                   "Document whether ITEMS may contain negative values.")))
            (cl-letf (((symbol-function 'read-from-minibuffer)
                       (lambda (&rest _) (pop answers))))
              (goto-char (point-min))
              (search-forward "(+ subtotal item)")
              (let ((end (point))
                    (start (match-beginning 0)))
                (goto-char start)
                (push-mark end nil t)
                (setq mark-active t)
                (annotate-annotate))
              (deactivate-mark)
              (goto-char (point-min))
              (search-forward "items")
              (backward-char 2)
              (annotate-annotate)))
          (setq
           first-session
           (mapcar
            (lambda (annotation)
              (list
               (annotate-beginning-of-annotation annotation)
               (annotate-ending-of-annotation annotation)
               (annotate-annotated-text annotation)
               (annotate-annotation-string annotation)))
            (sort
             (annotate-describe-annotations)
             (lambda (a b)
               (< (annotate-beginning-of-annotation a)
                  (annotate-beginning-of-annotation b))))))
          (kill-buffer source-buffer))
        (setq source-buffer nil)

        ;; Simulate another tool adding a header while Annotate is not active.
        ;; On the next visit the notes must be found from their saved text,
        ;; shifted to the new positions, and navigable with the documented keys.
        (with-temp-buffer
          (insert-file-contents source-file)
          (goto-char (point-min))
          (insert ";;; invoice calculations\n")
          (write-region (point-min) (point-max) source-file nil 'silent))
        (setq source-buffer (find-file-noselect source-file))
        (with-current-buffer source-buffer
          (emacs-lisp-mode)
          (annotate-mode 1)
          (let ((restored
                 (mapcar
                  (lambda (annotation)
                    (list
                     (annotate-beginning-of-annotation annotation)
                     (annotate-ending-of-annotation annotation)
                     (annotate-annotated-text annotation)
                     (annotate-annotation-string annotation)))
                  (sort
                   (annotate-describe-annotations)
                   (lambda (a b)
                     (< (annotate-beginning-of-annotation a)
                        (annotate-beginning-of-annotation b))))))
                visits)
            (goto-char (point-min))
            (annotate-goto-next-annotation)
            (push
             (list
              (line-number-at-pos)
              (annotate-annotation-get-annotation-text
               (annotate-annotation-at (point))))
             visits)
            (annotate-goto-next-annotation)
            (push
             (list
              (line-number-at-pos)
              (annotate-annotation-get-annotation-text
               (annotate-annotation-at (point))))
             visits)
            (setq
             reopened-session
             (list
              restored
              (nreverse visits)
              (buffer-substring-no-properties
               (point-min) (point-max))
              (file-exists-p annotate-file)))))
        (list first-session reopened-session))
    (when (buffer-live-p source-buffer)
      (with-current-buffer source-buffer
        (setq annotate-autosave nil)
        (annotate-mode -1))
      (kill-buffer source-buffer))
    (when (file-directory-p root)
      (delete-directory root t))))
"##,
        expect![[
            r#"OK (((23 28 "items" "Document whether ITEMS may contain negative values.") (107 124 "(+ subtotal item)" "This accumulator update needs an overflow policy.")) (((48 53 "items" "Document whether ITEMS may contain negative values.") (132 149 "(+ subtotal item)" "This accumulator update needs an overflow policy.")) ((2 "Document whether ITEMS may contain negative values.") (5 "This accumulator update needs an overflow policy.")) ";;; invoice calculations\n(defun invoice-total (items)\n  (let ((subtotal 0))\n    (dolist (item items subtotal)\n      (setq subtotal (+ subtotal item)))))\n" t))"#
        ]],
    )
}

fn reviewer_edits_styles_hides_and_deletes_a_note_through_documented_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "reviewer_edits_styles_hides_and_deletes_a_note_through_documented_commands",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "annotate-edit-review"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source-file (expand-file-name "parser.el" root))
       (annotate-file (expand-file-name "review-notes.el" root))
       (annotate-use-echo-area t)
       (annotate-use-messages nil)
       (annotate-warn-if-hash-mismatch nil)
       (annotate-database-confirm-deletion nil)
       (annotate-annotation-confirm-deletion nil)
       (annotate-autosave t)
       source-buffer
       initial-style
       edited-state
       deleted-state)
  (unwind-protect
      (progn
        (make-directory root t)
        (with-temp-file source-file
          (insert
           "(defun parse-record (line)\n"
           "  (split-string line \",\" t))\n"))
        (setq source-buffer (find-file-noselect source-file))
        (with-current-buffer source-buffer
          (emacs-lisp-mode)
          (annotate-mode 1)
          (goto-char (point-min))
          (search-forward "split-string")
          (backward-word)
          (let ((answers
                 '("Handle quoted commas before shipping."
                   "Use a CSV parser so quoted commas remain inside fields.")))
            (cl-letf (((symbol-function 'read-from-minibuffer)
                       (lambda (&rest _) (pop answers))))
              (annotate-annotate)
              (annotate-annotate)))
          (let ((annotation (annotate-annotation-at (point))))
            (setq
             initial-style
             (list
              (annotate-annotation-get-position annotation)
              (annotate-annotation-face annotation)
              (annotate-annotation-property-annotation-face annotation))))
          (annotate-change-annotation-colors)
          (annotate-change-annotation-text-position)
          (annotate-change-annotation-text-position)
          (annotate-toggle-annotation-text)
          (let ((annotation (annotate-annotation-at (point))))
            (setq
             edited-state
             (list
              (buffer-substring-no-properties
               (overlay-start annotation) (overlay-end annotation))
              (annotate-annotation-get-annotation-text annotation)
              (annotate-annotation-get-position annotation)
              (annotate-annotation-face annotation)
              (annotate-annotation-property-annotation-face annotation)
              (annotate-tail-overlay-hide-text-p
               (annotate-chain-last-ring
                (annotate-chain-at (point))))
              (length (annotate-load-annotation-data t))
              (file-exists-p annotate-file))))
          (annotate-delete-annotation)
          (setq
           deleted-state
           (list
            (annotate-annotations-exist-p)
            (file-exists-p annotate-file)
            (buffer-substring-no-properties
             (point-min) (point-max))
            (with-temp-buffer
              (insert-file-contents source-file)
              (buffer-string)))))
        (list initial-style edited-state deleted-state))
    (when (buffer-live-p source-buffer)
      (with-current-buffer source-buffer
        (setq annotate-autosave nil)
        (annotate-mode -1))
      (kill-buffer source-buffer))
    (when (file-directory-p root)
      (delete-directory root t))))
"##,
        expect![[
            r##"OK ((nil (:underline "#EEF192") (:background "#EEF192" :foreground "black")) ("split-string" "Use a CSV parser so quoted commas remain inside fields." :margin (:underline "#92EEF1") (:background "#92EEF1" :foreground "black") t 1 t) (nil nil "(defun parse-record (line)\n  (split-string line \",\" t))\n" "(defun parse-record (line)\n  (split-string line \",\" t))\n"))"##
        ]],
    )
}

fn code_review_annotations_integrate_as_real_lisp_comments_and_clear_the_database()
-> ParityBatchCase {
    ParityBatchCase::value(
        "code_review_annotations_integrate_as_real_lisp_comments_and_clear_the_database",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "annotate-integrated-review"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source-file (expand-file-name "totals.el" root))
       (annotate-file (expand-file-name "review-notes.el" root))
       (annotate-use-echo-area t)
       (annotate-use-messages nil)
       (annotate-warn-if-hash-mismatch nil)
       (annotate-database-confirm-deletion nil)
       (annotate-autosave t)
       source-buffer
       before-integration
       after-integration)
  (unwind-protect
      (progn
        (make-directory root t)
        (with-temp-file source-file
          (insert
           "(defun total-with-tax (subtotal rate)\n"
           "  (* subtotal (+ 1 rate)))\n"))
        (setq source-buffer (find-file-noselect source-file))
        (with-current-buffer source-buffer
          (emacs-lisp-mode)
          (transient-mark-mode 1)
          (annotate-mode 1)
          (let ((answers
                 '("Define whether RATE is a fraction or a percentage."
                   "Name the rounding rule used for currency.")))
            (cl-letf (((symbol-function 'read-from-minibuffer)
                       (lambda (&rest _) (pop answers))))
              (goto-char (point-min))
              (search-forward "rate")
              (backward-word)
              (annotate-annotate)
              (goto-char (point-min))
              (search-forward "(* subtotal (+ 1 rate))")
              (let ((end (point))
                    (start (match-beginning 0)))
                (goto-char start)
                (push-mark end nil t)
                (setq mark-active t)
                (annotate-annotate))))
          (setq
           before-integration
           (list
            (buffer-modified-p)
            (length (annotate-describe-annotations))
            (file-exists-p annotate-file)
            (with-temp-buffer
              (insert-file-contents source-file)
              (buffer-string))))
          (deactivate-mark)
          (annotate-integrate-annotations)
          (save-buffer)
          (setq
           after-integration
           (list
            (buffer-substring-no-properties
             (point-min) (point-max))
            (annotate-annotations-exist-p)
            (buffer-modified-p)))
          ;; Killing an enabled annotated buffer is the documented persistence
          ;; boundary; with no overlays left it removes the now-empty database.
          (kill-buffer source-buffer))
        (setq source-buffer nil)
        (list
         before-integration
         after-integration
         (file-exists-p annotate-file)
         (with-temp-buffer
           (insert-file-contents source-file)
           (buffer-string))))
    (when (buffer-live-p source-buffer)
      (with-current-buffer source-buffer
        (setq annotate-autosave nil)
        (annotate-mode -1))
      (kill-buffer source-buffer))
    (when (file-directory-p root)
      (delete-directory root t))))
"##,
        expect![[
            r#"OK ((nil 2 t "(defun total-with-tax (subtotal rate)\n  (* subtotal (+ 1 rate)))\n") ("(defun total-with-tax (subtotal rate)\n;                               ~~~~\n; ANNOTATION: \n;Define whether RATE is a fraction or a percentage.\n  (* subtotal (+ 1 rate)))\n; ~~~~~~~~~~~~~~~~~~~~~~~\n; ANNOTATION: \n;Name the rounding rule used for currency.\n" nil nil) nil "(defun total-with-tax (subtotal rate)\n;                               ~~~~\n; ANNOTATION: \n;Define whether RATE is a fraction or a percentage.\n  (* subtotal (+ 1 rate)))\n; ~~~~~~~~~~~~~~~~~~~~~~~\n; ANNOTATION: \n;Name the rounding rule used for currency.\n")"#
        ]],
    )
}

fn review_thread_and_filtered_summary_render_saved_notes_from_real_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "review_thread_and_filtered_summary_render_saved_notes_from_real_files",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "annotate-threaded-review"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source-file (expand-file-name "checkout.el" root))
       (annotate-file (expand-file-name "review-notes.el" root))
       (annotate-use-echo-area t)
       (annotate-use-messages nil)
       (annotate-warn-if-hash-mismatch nil)
       (annotate-database-confirm-deletion nil)
       (annotate-autosave t)
       (annotate-summary-ask-query nil)
       source-buffer
       thread-text
       summary-text)
  (unwind-protect
      (progn
        (make-directory root t)
        (with-temp-file source-file
          (insert
           "(defun charge-card (card cents)\n"
           "  (gateway-charge card cents))\n"
           "\n"
           "(defun send-receipt (address)\n"
           "  (mail-send address))\n"))
        (setq source-buffer (find-file-noselect source-file))
        (with-current-buffer source-buffer
          (emacs-lisp-mode)
          (annotate-mode 1)
          (let ((answers
                 '("SECURITY: redact card data from gateway errors."
                   "OPERATIONS: retry transient mail failures.")))
            (cl-letf (((symbol-function 'read-from-minibuffer)
                       (lambda (&rest _) (pop answers))))
              (goto-char (point-min))
              (search-forward "gateway-charge")
              (backward-word)
              (annotate-annotate)
              (goto-char (point-min))
              (search-forward "mail-send")
              (backward-word)
              (annotate-annotate)))

          (goto-char (point-min))
          (search-forward "gateway-charge")
          (backward-word)
          (let ((answers
                 '("reviewer@example.test"
                   "Agreed; sanitize the exception before logging.")))
            (cl-letf (((symbol-function 'read-from-minibuffer)
                       (lambda (&rest _) (pop answers))))
              (annotate-reply-to)))
          (annotate-show-thread-at-point)
          (setq
           thread-text
           (with-current-buffer annotate-thread-buffer-name
             (buffer-substring-no-properties
              (point-min) (point-max))))

          ;; Exercise the documented summary query against the real saved
          ;; database, rather than testing the lexer or parser in isolation.
          (annotate-show-annotation-summary
           "checkout\\.el and SECURITY"
           nil
           nil)
          (setq
           summary-text
           (with-current-buffer annotate-summary-buffer-name
             (buffer-substring-no-properties
              (point-min) (point-max))))
          (list
           thread-text
           summary-text
           (length
            (annotate-annotations-from-dump
             (car (annotate-load-annotation-data t))))
           (buffer-substring-no-properties
            (point-min) (point-max)))))
    (when (get-buffer annotate-thread-buffer-name)
      (kill-buffer annotate-thread-buffer-name))
    (when (get-buffer annotate-summary-buffer-name)
      (kill-buffer annotate-summary-buffer-name))
    (when (buffer-live-p source-buffer)
      (with-current-buffer source-buffer
        (setq annotate-autosave nil)
        (annotate-mode -1))
      (kill-buffer source-buffer))
    (when (file-directory-p root)
      (delete-directory root t))))
"##,
        expect![[
            r#"OK ("┏\n┃gateway-charge\n┗\nSECURITY: redact card data from gateway errors.\n│  ✏️add reply\n│  \n╰▶from: reviewer@example.test\n   Agreed; sanitize the exception before logging.\n    \n   ❌delete ✏️add reply\n  \n" "* File: [ORACLE-SANDBOX]/annotate-threaded-review/checkout.el\n\n** Annotated text: \"gateway-charge\"\n    SECURITY: redact card data from gateway errors.\n\n      ❌delete\n      📝replace\n      🧵show thread\n\n" 3 "(defun charge-card (card cents)\n  (gateway-charge card cents))\n\n(defun send-receipt (address)\n  (mail-send address))\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        review_notes_survive_reopen_and_follow_their_code_after_an_external_edit(),
        reviewer_edits_styles_hides_and_deletes_a_note_through_documented_commands(),
        code_review_annotations_integrate_as_real_lisp_comments_and_clear_the_database(),
        review_thread_and_filtered_summary_render_saved_notes_from_real_files(),
    ]
}
