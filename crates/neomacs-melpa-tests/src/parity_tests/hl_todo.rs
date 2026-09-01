use std::time::Duration;

use expect_test::expect;

use crate::{COMPAT_GNU_ELPA_PIN, COND_LET_MELPA_PIN, CachedMelpaOracle, HL_TODO_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HL_TODO_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HL_TODO_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'flymake)
(require 'hl-todo)

(defun neomacs-hl-todo-test-fontify ()
  "Fully fontify the current buffer after enabling Hl-Todo."
  (font-lock-mode 1)
  (hl-todo-mode 1)
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-hl-todo-test-point ()
  "Describe point after a public Hl-Todo navigation command."
  (list :line (line-number-at-pos)
        :column (current-column)
        :point (point)
        :face-before-point
        (or (get-text-property (1- (point)) 'face)
            (get-text-property (1- (point)) 'font-lock-face))))

(defun neomacs-hl-todo-test-face (needle)
  "Return the visible face on the first occurrence of NEEDLE."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (let ((begin (match-beginning 0)))
      (or (get-text-property begin 'face)
          (get-text-property begin 'font-lock-face)))))

(defun neomacs-hl-todo-test-navigation-records ()
  "Visit all accepted annotations using the public next command."
  (goto-char (point-min))
  (let (records done)
    (while (not done)
      (condition-case nil
          (progn
            (hl-todo-next 1)
            (push (neomacs-hl-todo-test-point) records))
        (user-error (setq done t))))
    (nreverse records)))
"##;

fn hl_todo_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HL_TODO_MELPA_PIN, "hl-todo.el")
        .expect("prepare revision-pinned Hl-Todo source below ./tmp")
        .with_melpa_dependency(COND_LET_MELPA_PIN)
        .expect("prepare revision-pinned Cond-Let dependency below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare revision-pinned Compat dependency below ./tmp")
        .with_prelude(HL_TODO_TEST_PRELUDE)
        .with_timeout(HL_TODO_TEST_TIMEOUT)
}

fn source_annotations_are_fontified_and_navigated_without_code_false_positives() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert
   (concat
    "(defun deploy-release ()\n"
    "  (let ((status \"FIXME: notify operator\"))\n"
    "    ;; TODO: validate signed artifact\n"
    "    (publish status)))\n"
    "(setq TODO-state 'ready)\n"
    ";; todo: lowercase note\n"
    ";; TODOISH: prefix collision\n"))
  (neomacs-hl-todo-test-fontify)
  (goto-char (point-min))
  (hl-todo-next 1)
  (let ((first (neomacs-hl-todo-test-point)))
    (hl-todo-next 1)
    (let* ((second (neomacs-hl-todo-test-point))
           (past-end
            (condition-case err
                (progn (hl-todo-next 1) :unexpected-success)
              (user-error (error-message-string err)))))
      (hl-todo-previous 1)
      (let ((previous (neomacs-hl-todo-test-point)))
        (goto-char (point-max))
        (let ((hl-todo-wrap-movement t))
          (hl-todo-next 1))
        (list :forward (list first second)
              :previous previous
              :wrapped (neomacs-hl-todo-test-point)
              :past-end past-end
              :code-identifier-face
              (neomacs-hl-todo-test-face "TODO-state")
              :lowercase-face
              (neomacs-hl-todo-test-face "todo: lowercase")
              :prefix-collision-face
              (neomacs-hl-todo-test-face "TODOISH"))))))
"##;
    let expected = expect![[
        r####"OK (:forward ((:line 2 :column 22 :point 48 :face-before-point #1=((:foreground "#cc9393") hl-todo font-lock-string-face)) (:line 3 :column 11 :point 80 :face-before-point ((:foreground "#cc9393") hl-todo font-lock-comment-face))) :previous (:line 2 :column 22 :point 48 :face-before-point #1#) :wrapped (:line 2 :column 22 :point 48 :face-before-point #1#) :past-end "No more matches" :code-identifier-face nil :lowercase-face font-lock-comment-face :prefix-collision-face font-lock-comment-face)"####
    ]];
    ParityBatchCase::value(
        "source_annotations_are_fontified_and_navigated_without_code_false_positives",
        elisp_form,
        expected,
    )
}

fn inserting_required_annotations_respects_source_context() -> ParityBatchCase {
    let elisp_form = r##"
(let ((hl-todo-highlight-punctuation ":")
      (hl-todo-require-punctuation t))
  (list
   :after-code
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert "(defun ship ()\n  (publish artifact))\n")
     (goto-char (point-min))
     (search-forward "artifact))")
     (hl-todo-insert "TODO")
     (list :point (point)
           :buffer (buffer-substring-no-properties (point-min) (point-max))))
   :before-code
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert "(defun ship ()\n  (publish artifact))\n")
     (goto-char (point-min))
     (search-forward "publish")
     (hl-todo-insert "FIXME")
     (list :point (point)
           :line (line-number-at-pos)
           :buffer (buffer-substring-no-properties (point-min) (point-max))))
   :inside-comment
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert ";; Reviewer requested evidence before release\n")
     (goto-char (point-min))
     (search-forward "Reviewer")
     (hl-todo-insert "NOTE")
     (list :point (point)
           :buffer (buffer-substring-no-properties (point-min) (point-max))))))
"##;
    let expected = expect![[
        r####"OK (:after-code (:point 46 :buffer "(defun ship ()\n  (publish artifact)) ; TODO: \n") :before-code (:point 27 :line 2 :buffer "(defun ship ()\n  ;; FIXME:\n  (publish artifact))\n") :inside-comment (:point 18 :buffer ";; Reviewer NOTE: requested evidence before release\n"))"####
    ]];
    ParityBatchCase::value(
        "inserting_required_annotations_respects_source_context",
        elisp_form,
        expected,
    )
}

fn flymake_reports_actionable_annotation_ranges_and_text() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert
   (concat
    "(defun deploy-release ()\n"
    "  ;; TODO: verify artifact checksum\n"
    "  (message \"FIXME: notify operator\")\n"
    "  (setq TODO-state 'ready)\n"
    "  ;; Maintainer NOTE: rotate signing key\n"
    "  (publish))\n"))
  (hl-todo-mode 1)
  (let (reported)
    (hl-todo-flymake (lambda (diagnostics) (setq reported diagnostics)))
    (let ((active
           (mapcar
            (lambda (diagnostic)
              (let ((begin (flymake-diagnostic-beg diagnostic))
                    (end (flymake-diagnostic-end diagnostic)))
                (list :type (flymake-diagnostic-type diagnostic)
                      :start (list (line-number-at-pos begin)
                                   (save-excursion
                                     (goto-char begin)
                                     (current-column)))
                      :end (list (line-number-at-pos end)
                                 (save-excursion
                                   (goto-char end)
                                   (current-column)))
                      :text (flymake-diagnostic-text
                             diagnostic '(message)))))
            reported)))
      (hl-todo-mode -1)
      (setq reported :not-reported)
      (hl-todo-flymake (lambda (diagnostics) (setq reported diagnostics)))
      (list :active active
            :disabled reported
            :category (get 'hl-todo-flymake 'flymake-category)
            :type-name (get 'hl-todo-flymake 'flymake-type-name)))))
"##;
    let expected = expect![[
        r####"OK (:active ((:type hl-todo-flymake :start (2 5) :end (2 35) :text "TODO: verify artifact checksum") (:type hl-todo-flymake :start (3 2) :end (3 36) :text "(message \"FIXME: notify operator\")") (:type hl-todo-flymake :start (5 5) :end (5 40) :text "Maintainer NOTE: rotate signing key")) :disabled nil :category flymake-note :type-name "todo")"####
    ]];
    ParityBatchCase::value(
        "flymake_reports_actionable_annotation_ranges_and_text",
        elisp_form,
        expected,
    )
}

fn release_checklists_honor_punctuation_faces_and_delimiter_policy() -> ParityBatchCase {
    let elisp_form = r##"
(list
 :punctuated-checklist
 (with-temp-buffer
   (let ((hl-todo-keyword-faces
          '(("TODO" . "#112233")
            ("BLOCKED" . font-lock-warning-face)))
         (hl-todo-highlight-punctuation ":!")
         (hl-todo-require-punctuation t))
     (text-mode)
     (insert
      (concat
       "Release checklist\n"
       "TODO: build signed artifacts\n"
       "TODO missing required punctuation\n"
       "BLOCKED!! waiting for approval\n"
       "todo: lowercase note\n"))
     (neomacs-hl-todo-test-fontify)
     (neomacs-hl-todo-test-navigation-records)))
 :delimiter-policy
 (mapcar
  (lambda (delimiter)
    (with-temp-buffer
      (let ((hl-todo-keyword-faces '(("TODO" . "#112233")))
            (hl-todo-keyword-delimiters delimiter))
        (emacs-lisp-mode)
        (insert ";; foo-TODO_bar xTODOx TODO\n")
        (neomacs-hl-todo-test-fontify)
        (list delimiter
              (mapcar (lambda (record)
                        (list (plist-get record :line)
                              (plist-get record :column)))
                      (neomacs-hl-todo-test-navigation-records))))))
  '(symbol word nil)))
"##;
    let expected = expect![[
        r####"OK (:punctuated-checklist ((:line 2 :column 5 :point 24 :face-before-point ((:foreground "#112233") hl-todo)) (:line 4 :column 9 :point 91 :face-before-point (font-lock-warning-face))) :delimiter-policy ((symbol ((1 27))) (word ((1 11) (1 27))) (nil ((1 11) (1 21) (1 27)))))"####
    ]];
    ParityBatchCase::value(
        "release_checklists_honor_punctuation_faces_and_delimiter_policy",
        elisp_form,
        expected,
    )
}

fn global_mode_tracks_existing_and_new_editing_buffers_only() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (require 'org)
  (global-hl-todo-mode -1)
  (let ((source (generate-new-buffer "*hl-todo-source*"))
        (notes (generate-new-buffer "*hl-todo-notes*"))
        (agenda (generate-new-buffer "*hl-todo-agenda*"))
        (status (generate-new-buffer "*hl-todo-status*"))
        (temporary (generate-new-buffer " *temp*hl-todo-release"))
        future)
    (unwind-protect
        (progn
          (with-current-buffer source (emacs-lisp-mode))
          (with-current-buffer notes (text-mode))
          (with-current-buffer agenda (org-mode))
          (with-current-buffer status (special-mode))
          (with-current-buffer temporary (emacs-lisp-mode))
          (global-hl-todo-mode 1)
          (let ((existing
                 (mapcar (lambda (buffer)
                           (with-current-buffer buffer
                             (list major-mode hl-todo-mode)))
                         (list source notes agenda status temporary))))
            (setq future (generate-new-buffer "*hl-todo-future-source*"))
            (with-current-buffer future
              (emacs-lisp-mode))
            (let ((new-buffer
                   (with-current-buffer future
                     (list major-mode hl-todo-mode))))
              (global-hl-todo-mode -1)
              (list :existing existing
                    :new-buffer new-buffer
                    :after-global-disable
                    (mapcar (lambda (buffer)
                              (with-current-buffer buffer hl-todo-mode))
                            (append (list source notes agenda status temporary)
                                    (list future)))))))
      (global-hl-todo-mode -1)
      (mapc (lambda (buffer)
              (when (buffer-live-p buffer) (kill-buffer buffer)))
            (append (list source notes agenda status temporary)
                    (and future (list future)))))))
"##;
    let expected = expect![[
        r####"OK (:existing ((emacs-lisp-mode t) (text-mode t) (org-mode nil) (special-mode nil) (emacs-lisp-mode nil)) :new-buffer (emacs-lisp-mode t) :after-global-disable (nil nil nil nil nil nil))"####
    ]];
    ParityBatchCase::value(
        "global_mode_tracks_existing_and_new_editing_buffers_only",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn revision_summary_hook_highlights_only_the_unwashed_tail() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((hl-todo-keyword-faces
         '(("TODO" . "#aa0000")
           ("DONE" . success)
           ("FIXME" . warning)))
        (hl-todo-highlight-punctuation ":")
        (hl-todo-require-punctuation t))
    (insert
     (concat
      "TODO: old heading already rendered\n"
      "Release 2.0\n"
      "TODO: sign artifacts\n"
      "DONE: publish checksums\n"
      "FIXME without required punctuation\n"))
    (goto-char (point-min))
    (forward-line 1)
    (hl-todo-search-and-highlight)
    (mapcar
     (lambda (needle)
       (goto-char (point-min))
       (search-forward needle)
       (let ((begin (match-beginning 0))
             (end (match-end 0))
             (punctuation (string-match ":" needle)))
         (list :text needle
               :keyword-face (get-text-property begin 'font-lock-face)
               :punctuation-face
               (and punctuation
                    (get-text-property (+ begin punctuation) 'font-lock-face))
               :tail-face (get-text-property (1- end) 'font-lock-face))))
     '("TODO: old" "TODO: sign" "DONE: publish" "FIXME without"))))
"##;
    let expected = expect![[
        r####"OK ((:text "TODO: old" :keyword-face nil :punctuation-face nil :tail-face nil) (:text "TODO: sign" :keyword-face #1=((:foreground "#aa0000") hl-todo) :punctuation-face #1# :tail-face nil) (:text "DONE: publish" :keyword-face success :punctuation-face success :tail-face nil) (:text "FIXME without" :keyword-face nil :punctuation-face nil :tail-face nil))"####
    ]];
    ParityBatchCase::value(
        "revision_summary_hook_highlights_only_the_unwashed_tail",
        elisp_form,
        expected,
    )
}

fn occur_results_navigate_to_the_exact_superset_of_source_annotations() -> ParityBatchCase {
    let elisp_form = r##"
(let ((source (generate-new-buffer "*hl-todo-occur-source*"))
      occur-buffer)
  (unwind-protect
      (progn
        (with-current-buffer source
          (emacs-lisp-mode)
          (insert
           (concat
            "(setq TODO 'pending)\n"
            ";; TODO: validate artifact\n"
            "(message \"FIXME: notify operator\")\n"
            ";; todo: lowercase note\n"))
          (hl-todo-occur))
        (setq occur-buffer (get-buffer "*Occur*"))
        (with-current-buffer occur-buffer
          (let (targets)
            (goto-char (point-min))
            (while (not (eobp))
              (when (get-text-property (line-beginning-position) 'occur-target)
                (let ((marker (occur-mode-find-occurrence)))
                  (push
                   (with-current-buffer (marker-buffer marker)
                     (goto-char marker)
                     (list :line (line-number-at-pos)
                           :column (current-column)
                           :source-line
                           (buffer-substring-no-properties
                            (line-beginning-position) (line-end-position))))
                   targets)))
              (forward-line 1))
            (list :mode major-mode
                  :match-count (length targets)
                  :targets (nreverse targets)))))
    (when (buffer-live-p occur-buffer) (kill-buffer occur-buffer))
    (when (buffer-live-p source) (kill-buffer source))))
"##;
    let expected = expect![[
        r####"OK (:mode occur-mode :match-count 3 :targets ((:line 1 :column 6 :source-line "(setq TODO 'pending)") (:line 2 :column 3 :source-line ";; TODO: validate artifact") (:line 3 :column 10 :source-line "(message \"FIXME: notify operator\")")))"####
    ]];
    ParityBatchCase::value(
        "occur_results_navigate_to_the_exact_superset_of_source_annotations",
        elisp_form,
        expected,
    )
    .fresh_process()
}

#[test]
fn hl_todo_package_batch() {
    assert_oracle_batch_cases(
        hl_todo_oracle(),
        "hl-todo-package-batch",
        "Hl-Todo",
        &[
            source_annotations_are_fontified_and_navigated_without_code_false_positives(),
            inserting_required_annotations_respects_source_context(),
            flymake_reports_actionable_annotation_ranges_and_text(),
            release_checklists_honor_punctuation_faces_and_delimiter_policy(),
            global_mode_tracks_existing_and_new_editing_buffers_only(),
            revision_summary_hook_highlights_only_the_unwashed_tail(),
            occur_results_navigate_to_the_exact_superset_of_source_annotations(),
        ],
    );
}
