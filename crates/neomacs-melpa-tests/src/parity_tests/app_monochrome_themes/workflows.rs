use expect_test::expect;

use super::ParityBatchCase;

fn app_monochrome_dark_supports_a_real_edit_build_and_error_navigation_session() -> ParityBatchCase
{
    ParityBatchCase::value(
        "app_monochrome_dark_supports_a_real_edit_build_and_error_navigation_session",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "app-monochrome-dark-development"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "src/revenue.el" root))
       (checker (expand-file-name "tools/check-revenue" root))
       (default-directory root)
       source-buffer
       compilation-buffer
       source-faces
       compilation-faces
       diagnostics
       jumps
       result)
  (unwind-protect
      (progn
        (neomacs-app-monochrome-test-cleanup root)
        (make-directory (file-name-directory source) t)
        (make-directory (file-name-directory checker) t)
        (with-temp-file source
          (insert
           "(defun calculate-total (items)\n"
           "  \"Return the invoiced total for ITEMS.\"\n"
           "  (let ((tax-rate 0.20))\n"
           "    ;; Production tax policy.\n"
           "    (+ (apply #'+ items)\n"
           "       (* tax-rate (apply #'+ items)))))\n"
           "\n"
           "(message \"total=%s\" (calculate-total '(10 20)))\n"))
        (with-temp-file checker
          (insert
           "#!/bin/sh\n"
           "set -eu\n"
           "source=$1\n"
           "printf '%s:4:8: warning: tax policy needs review\\n' \"$source\"\n"
           "printf '%s:8:2: error: production logger is required\\n' \"$source\"\n"
           "exit 1\n"))
        (set-file-modes checker #o755)
        (setq source-buffer (find-file-noselect source))
        (switch-to-buffer source-buffer)
        (load-theme 'app-monochrome-themes-dark-theme t)
        (goto-char (point-min))
        (search-forward "0.20")
        (replace-match "0.21" t t)
        (indent-region (point-min) (point-max))
        (font-lock-ensure)
        (save-buffer)
        (setq source-faces
              (mapcar
               (lambda (request)
                 (neomacs-app-monochrome-test-face-at
                  (car request)
                  (cadr request)
                  (nth 2 request)))
               '(("defun" (:family :foreground :weight :slant))
                 ("calculate-total"
                  (:family :foreground :weight :slant))
                 ("Return the invoiced"
                  (:family :foreground :weight :slant))
                 ("let" (:family :foreground :weight :slant))
                 ("Production tax policy"
                  (:family :foreground :weight :slant))
                 ("total=%s" (:family :foreground :weight :slant)))))
        (let
            ((compilation-buffer-name-function
              (lambda (_mode)
                "*app-monochrome-build*")))
          (setq compilation-buffer
                (compile
                 (mapconcat
                  #'shell-quote-argument
                  (list checker source)
                  " "))))
        (let ((process (get-buffer-process compilation-buffer)))
          (while
              (process-live-p process)
            (accept-process-output process 0.05))
          (accept-process-output process 0.05))
        (with-current-buffer compilation-buffer
          (font-lock-ensure)
          (setq diagnostics
                (mapcar
                 (lambda (needle)
                   (goto-char (point-min))
                   (search-forward needle)
                   (neomacs-app-monochrome-test-line))
                 '("warning:" "error:"))
                compilation-faces
                (mapcar
                 (lambda (request)
                   (neomacs-app-monochrome-test-face-at
                    (car request)
                    (cadr request)))
                 '(("4:8" (:family :foreground :weight))
                   ("warning:" (:foreground :background :weight))
                   ("8:2" (:family :foreground :weight))
                   ("error:" (:foreground :background :weight))))))
        (switch-to-buffer compilation-buffer)
        (setq next-error-last-buffer compilation-buffer)
        (next-error 1 t)
        (with-current-buffer source-buffer
          (push
           (list
            (file-relative-name buffer-file-name root)
            (line-number-at-pos)
            (current-column)
            (neomacs-app-monochrome-test-line))
           jumps))
        (switch-to-buffer compilation-buffer)
        (next-error 1)
        (with-current-buffer source-buffer
          (push
           (list
            (file-relative-name buffer-file-name root)
            (line-number-at-pos)
            (current-column)
            (neomacs-app-monochrome-test-line))
           jumps))
        (setq result
              (list
               :theme custom-enabled-themes
               :source-faces source-faces
               :compilation-faces compilation-faces
               :diagnostics diagnostics
               :jumps (nreverse jumps)
               :disk
               (neomacs-app-monochrome-test-file-string source))))
    (neomacs-app-monochrome-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:theme (app-monochrome-themes-dark-theme) :source-faces (("defun" font-lock-keyword-face ((:family "Ubuntu Mono") (:foreground "unspecified-fg") (:weight bold) (:slant normal))) ("calculate-total" font-lock-function-name-face ((:family "IBM Plex Mono") (:foreground "unspecified-fg") (:weight regular) (:slant italic))) ("Return the invoiced" font-lock-doc-face ((:family "IBM Plex Mono") (:foreground "grey62") (:weight regular) (:slant italic))) ("let" font-lock-keyword-face ((:family "Ubuntu Mono") (:foreground "unspecified-fg") (:weight bold) (:slant normal))) ("Production tax policy" font-lock-comment-face ((:family "UbuntuMono Nerd Font") (:foreground "#aaa") (:weight regular) (:slant normal))) ("total=%s" font-lock-string-face ((:family "IBM Plex Mono") (:foreground "grey62") (:weight regular) (:slant normal)))) :compilation-faces (("4:8" (compilation-line-number underline) ((:family "Ubuntu Mono") (:foreground "unspecified-fg") (:weight bold))) ("warning:" (underline) ((:foreground "unspecified-fg") (:background "unspecified-bg") (:weight regular))) ("8:2" (compilation-line-number underline) ((:family "Ubuntu Mono") (:foreground "unspecified-fg") (:weight bold))) ("error:" (underline) ((:foreground "unspecified-fg") (:background "unspecified-bg") (:weight regular)))) :diagnostics ("[ORACLE-SANDBOX]/app-monochrome-dark-development/src/revenue.el:4:8: warning: tax policy needs review" "[ORACLE-SANDBOX]/app-monochrome-dark-development/src/revenue.el:8:2: error: production logger is required") :jumps (("src/revenue.el" 4 7 "    ;; Production tax policy.") ("src/revenue.el" 8 1 "(message \"total=%s\" (calculate-total '(10 20)))")) :disk "(defun calculate-total (items)\n  \"Return the invoiced total for ITEMS.\"\n  (let ((tax-rate 0.21))\n    ;; Production tax policy.\n    (+ (apply #'+ items)\n       (* tax-rate (apply #'+ items)))))\n\n(message \"total=%s\" (calculate-total '(10 20)))\n")"##
        ]],
    )
}

fn app_monochrome_light_supports_reviewing_folding_and_updating_a_real_org_plan() -> ParityBatchCase
{
    ParityBatchCase::value(
        "app_monochrome_light_supports_reviewing_folding_and_updating_a_real_org_plan",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "app-monochrome-light-writing"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (plan (expand-file-name "planning/release.org" root))
       (default-directory root)
       plan-buffer
       todo-action
       fold-action
       reveal-action
       folded
       revealed
       result)
  (unwind-protect
      (progn
        (neomacs-app-monochrome-test-cleanup root)
        (make-directory (file-name-directory plan) t)
        (with-temp-file plan
          (insert
           "#+title: Parity release\n"
           "\n"
           "* TODO Ship compatibility release\n"
           "  Review the [[https://example.test/report][compatibility report]] and =release-2026= tag.\n"
           "** DONE Verify package behavior\n"
           "   #+begin_src emacs-lisp\n"
           "   (message \"parity ready\")\n"
           "   #+end_src\n"
           "\n"
           "| Check          | Owner |\n"
           "|----------------+-------|\n"
           "| GNU comparison | team  |\n"))
        (setq plan-buffer (find-file-noselect plan))
        (switch-to-buffer plan-buffer)
        (setq-local org-log-done nil)
        (load-theme 'app-monochrome-themes-light-theme t)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "TODO")
        (let ((todo-start (match-beginning 0))
              (todo-end (match-end 0)))
          (setq todo-action
                (condition-case error
                    (progn
          (org-todo 'done)
                      (list
                       :updated
                       (neomacs-app-monochrome-test-line)))
                  (error
                   (goto-char todo-start)
                   (delete-region todo-start todo-end)
                   (insert "DONE")
                   (list :error error)))))
        (goto-char (point-max))
        (insert
         "\n* Notes\n"
         "  The release owner approved the compatibility report.\n")
        (indent-region (point-min) (point-max))
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "Ship compatibility release")
        (beginning-of-line)
        (let
            ((body-position
              (save-excursion
                (forward-line 1)
                (point))))
          (setq fold-action
                (condition-case error
                    (progn
                      (outline-hide-subtree)
                      :folded)
                  (error
                   (put-text-property
                    body-position
                    (point-max)
                    'invisible
                    'outline)
                   (list :error error))))
          (setq folded
                (list
                 (neomacs-app-monochrome-test-line)
                 (invisible-p body-position)))
          (setq reveal-action
                (condition-case error
                    (progn
                      (outline-show-subtree)
                      :revealed)
                  (error
                   (remove-text-properties
                    body-position
                    (point-max)
                    '(invisible nil))
                   (list :error error))))
          (setq revealed
                (list
                 (neomacs-app-monochrome-test-line)
                 (invisible-p body-position))))
        (save-buffer)
        (font-lock-ensure)
        (setq result
              (list
               :theme custom-enabled-themes
               :todo-action todo-action
               :fold-action fold-action
               :reveal-action reveal-action
               :folded folded
               :revealed revealed
               :faces
               (mapcar
                (lambda (request)
                  (neomacs-app-monochrome-test-face-at
                   (car request)
                   (cadr request)
                   (nth 2 request)))
                '(("Parity release"
                   (:foreground :background :weight :height))
                  ("DONE"
                   (:foreground :background :weight :box)
                   1)
                  ("compatibility report"
                   (:foreground :background :underline :weight)
                   1)
                  ("release-2026"
                   (:family :foreground :background :weight)
                   1)
                  ("message"
                   (:family :foreground :background :weight)
                   1)
                  ("Owner"
                   (:family :foreground :background :weight)
                   1)
                  ("Notes"
                   (:foreground :background :weight :height)
                   1)))
               :point
               (list
                (line-number-at-pos)
                (current-column)
                (neomacs-app-monochrome-test-line))
               :modified (buffer-modified-p)
               :disk
               (neomacs-app-monochrome-test-file-string plan))))
    (neomacs-app-monochrome-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:theme (app-monochrome-themes-light-theme) :todo-action (:updated "* DONE Ship compatibility release") :fold-action :folded :reveal-action :revealed :folded ("* DONE Ship compatibility release" 2) :revealed ("* DONE Ship compatibility release" nil) :faces (("Parity release" org-document-title ((:foreground "grey12") (:background "white") (:weight bold) (:height 98))) ("DONE" (org-done org-level-1) ((:foreground "black") (:background "#0bf") (:weight regular) (:box 1))) ("compatibility report" org-link ((:foreground "#3c5c5c") (:background "white") (:underline t) (:weight regular))) ("release-2026" (org-verbatim) ((:family "IBM Plex Mono") (:foreground "grey25") (:background "grey95") (:weight bold))) ("message" (org-block) ((:family "VictorMono Nerd Font") (:foreground "grey12") (:background "white") (:weight regular))) ("Owner" org-table ((:family "VictorMono Nerd Font") (:foreground "Blue1") (:background "white") (:weight regular))) ("Notes" org-level-1 ((:foreground "grey12") (:background "white") (:weight regular) (:height 98)))) :point (3 0 "* DONE Ship compatibility release") :modified nil :disk "#+title: Parity release\n\n* DONE Ship compatibility release\nReview the [[https://example.test/report][compatibility report]] and =release-2026= tag.\n** DONE Verify package behavior\n#+begin_src emacs-lisp\n  (message \"parity ready\")\n#+end_src\n\n| Check          | Owner |\n|----------------+-------|\n| GNU comparison | team  |\n\n* Notes\nThe release owner approved the compatibility report.\n")"##
        ]],
    )
}

fn app_monochrome_switches_a_real_dired_session_and_restores_the_previous_display()
-> ParityBatchCase {
    ParityBatchCase::value(
        "app_monochrome_switches_a_real_dired_session_and_restores_the_previous_display",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "app-monochrome-theme-lifecycle"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source-directory (expand-file-name "src/" root))
       (obsolete (expand-file-name "obsolete.log" root))
       (readme (expand-file-name "README.md" root))
       (default-directory root)
       (requests
        '((default :family)
          (default :height)
          (default :background)
          (default :foreground)
          (dired-directory :weight)
          (dired-flagged :foreground)
          (dired-flagged :box)
          (warning :foreground)
          (link :foreground)))
       dired-buffer
       baseline
       dark
       light
       revealed-dark
       restored
       flagged
       unflagged
       theme-states
       goto-action
       flag-action
       unflag-action
       result)
  (unwind-protect
      (progn
        (neomacs-app-monochrome-test-cleanup root)
        (require 'dired)
        (require 'dired-aux)
        (make-directory source-directory t)
        (with-temp-file obsolete
          (insert "obsolete build output\n"))
        (with-temp-file readme
          (insert "# Compatibility project\n"))
        (setq dired-buffer (dired-noselect root "-al"))
        (switch-to-buffer dired-buffer)
        (setq baseline
              (neomacs-app-monochrome-test-palette requests))
        (load-theme 'app-monochrome-themes-dark-theme t)
        (push (copy-sequence custom-enabled-themes) theme-states)
        (setq goto-action
              (condition-case error
                  (progn
                    (dired-goto-file obsolete)
                    (list
                     :visited
                     (dired-get-filename 'no-dir t)))
                (error
                 (goto-char (point-min))
                 (search-forward "obsolete.log")
                 (beginning-of-line)
                 (list :error error))))
        (setq flag-action
              (condition-case error
                  (progn
                    (dired-flag-file-deletion 1)
                    :flagged)
                (error
                 (list :error error))))
        (font-lock-ensure)
        (save-excursion
          (goto-char (point-min))
          (search-forward "obsolete.log")
          (beginning-of-line)
          (setq flagged
                (list
                 (char-to-string (char-after))
                 (neomacs-app-monochrome-test-face-at
                  "obsolete.log"
                  '(:foreground :background :weight :box))
                 (neomacs-app-monochrome-test-face-at
                  "src"
                  '(:foreground :background :weight)))))
        (setq dark
              (neomacs-app-monochrome-test-palette requests))
        (load-theme 'app-monochrome-themes-light-theme t)
        (push (copy-sequence custom-enabled-themes) theme-states)
        (setq light
              (neomacs-app-monochrome-test-palette requests))
        (disable-theme 'app-monochrome-themes-light-theme)
        (push (copy-sequence custom-enabled-themes) theme-states)
        (setq revealed-dark
              (neomacs-app-monochrome-test-palette requests))
        (disable-theme 'app-monochrome-themes-dark-theme)
        (push (copy-sequence custom-enabled-themes) theme-states)
        (setq restored
              (neomacs-app-monochrome-test-palette requests))
        (goto-char (point-min))
        (search-forward "obsolete.log")
        (beginning-of-line)
        (setq unflag-action
              (condition-case error
                  (progn
                    (dired-unmark 1)
                    :unflagged)
                (error
                 (list :error error))))
        (save-excursion
          (goto-char (point-min))
          (search-forward "obsolete.log")
          (beginning-of-line)
          (setq unflagged
                (list
                 (char-to-string (char-after))
                 (file-name-nondirectory obsolete))))
        (setq result
              (list
               :file default-directory
               :goto-action goto-action
               :flag-action flag-action
               :unflag-action unflag-action
               :baseline baseline
               :dark dark
               :light light
               :revealed-dark revealed-dark
               :restored restored
               :flagged flagged
               :unflagged unflagged
               :theme-states (nreverse theme-states))))
    (neomacs-app-monochrome-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:file "[ORACLE-SANDBOX]/app-monochrome-theme-lifecycle/" :goto-action (:visited "obsolete.log") :flag-action :flagged :unflag-action :unflagged :baseline ((default :family "default") (default :height 1) (default :background "unspecified-bg") (default :foreground "unspecified-fg") (dired-directory :weight bold) (dired-flagged :foreground "unspecified-fg") (dired-flagged :box nil) (warning :foreground "unspecified-fg") (link :foreground "unspecified-fg")) :dark ((default :family "UbuntuMono Nerd Font") (default :height 98) (default :background "unspecified-bg") (default :foreground "unspecified-fg") (dired-directory :weight bold) (dired-flagged :foreground "Red") (dired-flagged :box (:line-width (2 . 2) :color "Red" :style released-button)) (warning :foreground "gold") (link :foreground "#5cacac")) :light ((default :family "default") (default :height 98) (default :background "white") (default :foreground "grey12") (dired-directory :weight bold) (dired-flagged :foreground "Red") (dired-flagged :box (:line-width (2 . 2) :color "Red" :style released-button)) (warning :foreground "red4") (link :foreground "#3c5c5c")) :revealed-dark ((default :family "UbuntuMono Nerd Font") (default :height 98) (default :background "unspecified-bg") (default :foreground "unspecified-fg") (dired-directory :weight bold) (dired-flagged :foreground "Red") (dired-flagged :box (:line-width (2 . 2) :color "Red" :style released-button)) (warning :foreground "gold") (link :foreground "#5cacac")) :restored ((default :family "default") (default :height 1) (default :background "unspecified-bg") (default :foreground "unspecified-fg") (dired-directory :weight bold) (dired-flagged :foreground "unspecified-fg") (dired-flagged :box nil) (warning :foreground "unspecified-fg") (link :foreground "unspecified-fg")) :flagged ("D" ("obsolete.log" dired-flagged ((:foreground "Red") (:background "unspecified-bg") (:weight bold) (:box (:line-width (2 . 2) :color "Red" :style released-button)))) ("src" dired-directory ((:foreground "unspecified-fg") (:background "unspecified-bg") (:weight bold)))) :unflagged (" " "obsolete.log") :theme-states ((app-monochrome-themes-dark-theme) (app-monochrome-themes-light-theme app-monochrome-themes-dark-theme) (app-monochrome-themes-dark-theme) nil))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        app_monochrome_dark_supports_a_real_edit_build_and_error_navigation_session(),
        app_monochrome_light_supports_reviewing_folding_and_updating_a_real_org_plan(),
        app_monochrome_switches_a_real_dired_session_and_restores_the_previous_display(),
    ]
}
