use expect_test::expect;

use super::ParityBatchCase;

fn agda_compiler_batch_renders_semantic_tokens_diagnostics_help_and_definition_links()
-> ParityBatchCase {
    ParityBatchCase::value(
        "agda_compiler_batch_renders_semantic_tokens_diagnostics_help_and_definition_links",
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert
   "module Checkout where\n"
   "\n"
   "data Card : Set where\n"
   "  valid : Card\n"
   "\n"
   "charge : Card → Nat\n"
   "charge card = authorize card\n")
  (set-buffer-modified-p nil)
  (setq buffer-undo-list nil)
  (let ((annotation-bindings
         '((keyword . font-lock-keyword-face)
           (module . font-lock-type-face)
           (datatype . font-lock-type-face)
           (constructor . font-lock-constant-face)
           (function . font-lock-function-name-face)
           (warning . warning)))
        (before-changes 0)
        (after-changes 0))
    (add-hook
     'before-change-functions
     (lambda (&rest _) (setq before-changes (1+ before-changes)))
     nil t)
    (add-hook
     'after-change-functions
     (lambda (&rest _) (setq after-changes (1+ after-changes)))
     nil t)
    (cl-labels
        ((bounds-after (token start)
           (save-excursion
             (goto-char start)
             (search-forward token)
             (list (- (point) (length token)) (point))))
         (describe-runs ()
           (let ((position (point-min))
                 runs)
             (while (< position (point-max))
               (let* ((end
                      (next-property-change
                        position (current-buffer) (point-max)))
                      (annotated
                       (get-text-property
                        position 'annotation-annotated)))
                 (when annotated
                   (push
                    (list
                     (buffer-substring-no-properties position end)
                     (get-text-property position 'annotation-faces)
                     (get-text-property
                      position 'annotation-token-based)
                     (get-text-property position 'help-echo)
                     (get-text-property position 'annotation-goto)
                     (get-text-property position 'mouse-face))
                    runs))
                 (setq position end)))
             (nreverse runs))))
      (let* ((module-keyword (bounds-after "module" (point-min)))
             (module-name (bounds-after "Checkout" (cadr module-keyword)))
             (data-keyword (bounds-after "data" (cadr module-name)))
             (card-declaration (bounds-after "Card" (cadr data-keyword)))
             (constructor (bounds-after "valid" (cadr card-declaration)))
             (charge-signature (bounds-after "charge" (cadr constructor)))
             (card-argument (bounds-after "Card" (cadr charge-signature)))
             (charge-clause (bounds-after "charge" (cadr card-argument)))
             (authorize-call (bounds-after "authorize" (cadr charge-clause)))
             (definition-target '("Library/Payments.agda" . 73))
             (commands
              (list
               (append module-keyword
                       '((keyword) t "Agda module declaration"))
               (append module-name
                       `((module) t nil ,definition-target))
               (append data-keyword
                       '((keyword) t "Datatype declaration"))
               (append card-declaration
                       '((datatype) t "Card type"))
               (append constructor
                       '((constructor) t "Card constructor"))
               (append charge-signature
                       `((function) t nil ,definition-target))
               (append card-argument
                       '((datatype) t "Expected argument type"))
               (append charge-clause
                       `((function) t nil ,definition-target))
               (append authorize-call
                       '((function) t "Authorization backend"))
               ;; A compiler warning overlaps the semantic function token,
               ;; exactly as an interactive Agda diagnostic does.
               (append authorize-call
                       '((warning) nil
                         "Authorization may fail for an expired card.")))))
        (apply
         #'annotation-load
         "Mouse-2: jump to the definition"
         t
         nil
         commands)
        (list
         (describe-runs)
         (buffer-substring-no-properties (point-min) (point-max))
         (buffer-modified-p)
         buffer-undo-list
         before-changes
         after-changes)))))
"##,
        expect![[
            r#"OK ((("module" (font-lock-keyword-face) t "Agda module declaration" nil highlight) ("Checkout" (font-lock-type-face) t "Mouse-2: jump to the definition" #1=("Library/Payments.agda" . 73) highlight) ("data" (font-lock-keyword-face) t "Datatype declaration" nil highlight) ("Card" (font-lock-type-face) t "Card type" nil highlight) ("valid" (font-lock-constant-face) t "Card constructor" nil highlight) ("charge" (font-lock-function-name-face) t "Mouse-2: jump to the definition" #1# highlight) ("Card" (font-lock-type-face) t "Expected argument type" nil highlight) ("charge" (font-lock-function-name-face) t "Mouse-2: jump to the definition" #1# highlight) ("authorize" (warning font-lock-function-name-face) t "Authorization may fail for an expired card." nil highlight)) "module Checkout where\n\ndata Card : Set where\n  valid : Card\n\ncharge : Card → Nat\ncharge card = authorize card\n" nil nil 0 0)"#
        ]],
    )
}

fn incremental_agda_reload_replaces_stale_tokens_but_preserves_a_non_token_review_note()
-> ParityBatchCase {
    ParityBatchCase::value(
        "incremental_agda_reload_replaces_stale_tokens_but_preserves_a_non_token_review_note",
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert
   "module Parser where\n"
   "\n"
   "parse : String → Result\n"
   "parse input = unsafeParse input\n")
  (let ((annotation-bindings
         '((keyword . font-lock-keyword-face)
           (module . font-lock-type-face)
           (function . font-lock-function-name-face)
           (unsafe . font-lock-warning-face)
           (review . font-lock-doc-face)))
        before-changes
        after-changes)
    (cl-labels
        ((bounds (token occurrence)
           (save-excursion
             (goto-char (point-min))
             (dotimes (_ occurrence)
               (search-forward token))
             (list (- (point) (length token)) (point))))
         (semantic-runs ()
           (let ((position (point-min))
                 runs)
             (while (< position (point-max))
               (let ((end
                      (next-property-change
                       position (current-buffer) (point-max))))
                 (when
                     (get-text-property
                      position 'annotation-annotated)
                   (push
                    (list
                     (buffer-substring-no-properties position end)
                     (get-text-property position 'annotation-faces)
                     (get-text-property
                      position 'annotation-token-based)
                     (get-text-property position 'help-echo))
                    runs))
                 (setq position end)))
             (nreverse runs))))
      (let* ((module-keyword (bounds "module" 1))
             (module-name (bounds "Parser" 1))
             (signature-name (bounds "parse" 1))
             (clause-name (bounds "parse" 2))
             (unsafe-call (bounds "unsafeParse" 1))
             (module-line-end
              (save-excursion
                (goto-char (point-min))
                (line-end-position))))
        (apply
         #'annotation-load
         "Jump to definition"
         t
         nil
         (list
          (append module-keyword '((keyword) t))
          (append module-name '((module) t))
          (append signature-name '((function) t))
          (append clause-name '((function) t))
          (append unsafe-call
                  '((unsafe) t "Unsafe parser implementation"))))
        ;; This is a human review note rather than compiler syntax, so a later
        ;; token refresh must not remove it.
        (annotation-load
         "Jump to definition"
         nil
         nil
         (list
          (point-min) module-line-end
          '(review) nil
          "Keep the public parser module small."))
        (let ((initial (semantic-runs)))
          ;; The programmer replaces the unsafe implementation before asking
          ;; Agda to reload the buffer.
          (goto-char (point-min))
          (search-forward "unsafeParse")
          (replace-match "safeParse" t t)
          (let ((modified-before-reload (buffer-modified-p))
                (undo-before-reload (copy-tree buffer-undo-list)))
            (setq before-changes 0
                  after-changes 0)
            (add-hook
             'before-change-functions
             (lambda (&rest _)
               (setq before-changes (1+ before-changes)))
             nil t)
            (add-hook
             'after-change-functions
             (lambda (&rest _)
               (setq after-changes (1+ after-changes)))
             nil t)
            (let* ((new-module-keyword (bounds "module" 1))
                   (new-module-name (bounds "Parser" 1))
                   (new-signature-name (bounds "parse" 1))
                   (new-clause-name (bounds "parse" 2))
                   (safe-call (bounds "safeParse" 1)))
              (apply
               #'annotation-load
               "Jump to definition"
               t
               nil
               (list
                (append new-module-keyword '((keyword) t))
                (append new-module-name '((module) t))
                (append new-signature-name '((function) t))
                (append new-clause-name '((function) t))
                (append safe-call
                        '((function) t "Total parser implementation"))))
              (list
               initial
               (semantic-runs)
               (buffer-substring-no-properties
                (point-min) (point-max))
               modified-before-reload
               (buffer-modified-p)
               (equal undo-before-reload buffer-undo-list)
               before-changes
               after-changes))))))))
"##,
        expect![[
            r#"OK ((("module" (font-lock-doc-face font-lock-keyword-face) t "Keep the public parser module small.") (" " #1=(font-lock-doc-face) nil "Keep the public parser module small.") ("Parse" (font-lock-doc-face font-lock-function-name-face font-lock-type-face) t "Keep the public parser module small.") ("r where" #1# nil "Keep the public parser module small.") ("parse" (font-lock-function-name-face) t nil) ("unsafeParse" (font-lock-warning-face) t "Unsafe parser implementation")) (("module" (font-lock-keyword-face) t nil) (" " #1# nil "Keep the public parser module small.") ("Parse" (font-lock-function-name-face font-lock-type-face) t nil) (" where" #1# nil "Keep the public parser module small.") ("parse" (font-lock-function-name-face) t nil) ("safeParse" (font-lock-function-name-face) t "Total parser implementation")) "module Parser where\n\nparse : String → Result\nparse input = safeParse input\n" t t t 0 0)"#
        ]],
    )
}

fn following_an_agda_definition_link_and_going_back_round_trips_real_project_files()
-> ParityBatchCase {
    ParityBatchCase::value(
        "following_an_agda_definition_link_and_going_back_round_trips_real_project_files",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "annotation-navigation-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (main-file (expand-file-name "Main.agda" root))
       (library-file (expand-file-name "Library.agda" root))
       (annotation-goto-stack nil)
       main-buffer
       library-buffer
       result)
  (make-directory root t)
  (with-temp-file main-file
    (insert
     "module Main where\n"
     "\n"
     "open import Library\n"
     "\n"
     "answer = double 21\n"))
  (with-temp-file library-file
    (insert
     "module Library where\n"
     "\n"
     "double : Nat → Nat\n"
     "double n = n + n\n"))
  (setq main-buffer (find-file-noselect main-file))
  (with-current-buffer main-buffer
    (let ((annotation-bindings
           '((module . font-lock-type-face)
             (function . font-lock-function-name-face))))
      (goto-char (point-min))
      (search-forward "double")
      (let* ((source-start (- (point) (length "double")))
             (source-end (point))
             (target-position
              (with-temp-buffer
                (insert-file-contents library-file)
                (goto-char (point-min))
                (search-forward "double")
                (- (point) (length "double")))))
        (annotation-load
         "Mouse-2: jump to definition"
         t
         nil
         (list
          source-start source-end
          '(function) t nil
          (cons library-file target-position)))
        (goto-char source-start)
        (let* ((link
                (get-text-property
                 source-start 'annotation-goto))
               (followed
                (annotation-goto-and-push
                 main-buffer source-start link)))
          (setq library-buffer (current-buffer))
          (let ((target-state
                 (list
                  followed
                  (file-relative-name buffer-file-name root)
                  (line-number-at-pos)
                  (current-column)
                  (buffer-substring-no-properties
                   (line-beginning-position)
                   (line-end-position))
                  (mapcar
                   (lambda (entry)
                     (cons
                      (file-relative-name (car entry) root)
                      (cdr entry)))
                   annotation-goto-stack))))
            (let ((back-result (annotation-go-back)))
              (setq
               result
               (list
                target-state
                back-result
                (file-relative-name buffer-file-name root)
                (line-number-at-pos)
                (current-column)
                (buffer-substring-no-properties
                 (line-beginning-position)
                 (line-end-position))
                annotation-goto-stack
                (annotation-go-back)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))))
  result)
"##,
        expect![[
            r#"OK ((t "Library.agda" 3 0 "double : Nat → Nat" (("Main.agda" . 50))) t "Main.agda" 5 9 "answer = double 21" nil nil "module Main where\n\nopen import Library\n\nanswer = double 21\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agda_compiler_batch_renders_semantic_tokens_diagnostics_help_and_definition_links(),
        incremental_agda_reload_replaces_stale_tokens_but_preserves_a_non_token_review_note(),
        following_an_agda_definition_link_and_going_back_round_trips_real_project_files(),
    ]
}
