use expect_test::expect;

use super::ParityBatchCase;

fn airline_themes_modeline_expression_renders_a_real_editing_buffer_with_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_modeline_expression_renders_a_real_editing_buffer_with_properties",
        r##"(progn
         (load-theme 'airline-doom-one t)
         (delete-other-windows)
         (let ((buffer (get-buffer-create "airline-practical.el")))
           (unwind-protect
               (progn
                 (set-window-buffer (selected-window) buffer)
                 (with-current-buffer buffer
                   (erase-buffer)
                   (insert
                    "(defun airline-practical (value)\n"
                    "  (+ value 42))\n")
                   (emacs-lisp-mode)
                   (set-buffer-modified-p t)
                   (setq buffer-file-coding-system 'utf-8-unix
                         powerline-selected-window
                         (selected-window))
                   (let* ((rendered
                           (eval
                            (airline-themes-mode-line-format)
                            t))
                          (plain
                           (substring-no-properties rendered))
                          (position 0)
                          intervals)
                     (while (< position (length rendered))
                       (let ((next
                              (next-property-change
                               position rendered
                               (length rendered))))
                         (push
                          (list
                           position next
                           (substring-no-properties
                            rendered position next)
                           (copy-tree
                            (text-properties-at
                             position rendered)))
                          intervals)
                         (setq position next)))
                     (list
                      plain
                      (length rendered)
                      (secure-hash
                       'sha256
                       (prin1-to-string
                        (nreverse intervals)))
                      (seq-filter
                       (lambda (interval)
                         (not
                          (string-empty-p
                           (string-trim
                            (nth 2 interval)))))
                       (nreverse intervals))))))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ("%*   %b   utf-8-unix %3p═%l/2ln :%3c " 42 "deffb17dcb403153adcc287d962912568e16a2d8a722abe9924f20235e2f3517" ((38 42 "%3c " (face (airline-normal-outer)))))"#
        ]],
    )
}

fn airline_themes_selected_and_inactive_windows_render_distinct_real_segments() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_selected_and_inactive_windows_render_distinct_real_segments",
        r##"(progn
         (load-theme 'airline-light t)
         (delete-other-windows)
         (let* ((left (selected-window))
                (right (split-window-right))
                (left-buffer
                 (get-buffer-create "airline-active.txt"))
                (right-buffer
                 (get-buffer-create "airline-inactive.txt")))
           (unwind-protect
               (progn
                 (set-window-buffer left left-buffer)
                 (set-window-buffer right right-buffer)
                 (with-current-buffer left-buffer
                   (erase-buffer)
                   (insert "active window\nsecond line\n")
                   (text-mode)
                   (setq buffer-file-coding-system
                         'utf-8-unix))
                 (with-current-buffer right-buffer
                   (erase-buffer)
                   (insert "inactive window\n")
                   (text-mode)
                   (setq buffer-file-coding-system
                         'utf-8-unix))
                 (select-window left)
                 (setq powerline-selected-window left)
                 (let ((active
                        (with-current-buffer left-buffer
                          (eval
                           (airline-themes-mode-line-format)
                           t))))
                   (select-window right)
                   (let ((inactive
                          (with-current-buffer right-buffer
                            (eval
                             (airline-themes-mode-line-format)
                             t))))
                     (list
                      (substring-no-properties active)
                      (substring-no-properties inactive)
                      (get-text-property 0 'face active)
                      (get-text-property 0 'face inactive)
                      (secure-hash
                       'sha256
                       (prin1-to-string
                        (text-properties-at 0 active)))
                      (secure-hash
                       'sha256
                       (prin1-to-string
                        (text-properties-at 0 inactive)))))))
             (delete-other-windows left)
             (kill-buffer left-buffer)
             (kill-buffer right-buffer))))"##,
        expect![[
            r#"OK ("%*   %b   utf-8-unix %3p═%l/2ln :%3c " "%*   %b   utf-8-unix %3p═%l/1ln :%3c " (airline-normal-outer) (airline-normal-outer) "c5f460e38f22fbad7f0db944f16cf0d5f1b835aa4412675e9d48af9aba84c9c6" "c5f460e38f22fbad7f0db944f16cf0d5f1b835aa4412675e9d48af9aba84c9c6")"#
        ]],
    )
}

fn airline_themes_inactive_visibility_customizations_remove_state_branch_and_eyebrowse()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_inactive_visibility_customizations_remove_state_branch_and_eyebrowse",
        r##"(progn
         (load-theme 'airline-dark t)
         (provide 'eyebrowse)
         (fset 'eyebrowse-mode-line-indicator
               (lambda () "[workspace-7]"))
         (delete-other-windows)
         (let* ((active-window (selected-window))
                (inactive-window (split-window-right))
                (buffer
                 (get-buffer-create
                  "airline-inactive-project.el")))
           (unwind-protect
               (progn
                 (set-window-buffer inactive-window buffer)
                 (with-current-buffer buffer
                   (erase-buffer)
                   (insert "(message \"project\")\n")
                   (emacs-lisp-mode)
                   (setq buffer-file-name
                         "/workspace/project/source.el"
                         vc-mode " Git:feature/palette"
                         buffer-file-coding-system
                         'utf-8-unix))
                 (setq powerline-selected-window
                       active-window)
                 (select-window inactive-window)
                 (let (visible hidden)
                   (setq airline-hide-state-on-inactive-buffers nil
                         airline-hide-eyebrowse-on-inactive-buffers nil
                         airline-hide-vc-branch-on-inactive-buffers nil)
                   (setq visible
                         (with-current-buffer buffer
                           (eval
                            (airline-themes-mode-line-format)
                            t)))
                   (setq airline-hide-state-on-inactive-buffers t
                         airline-hide-eyebrowse-on-inactive-buffers t
                         airline-hide-vc-branch-on-inactive-buffers t)
                   (setq hidden
                         (with-current-buffer buffer
                           (eval
                            (airline-themes-mode-line-format)
                            t)))
                   (list
                    (substring-no-properties visible)
                    (substring-no-properties hidden)
                    (- (length visible) (length hidden))
                    (string-match-p
                     "\\[workspace-7\\]"
                     (substring-no-properties visible))
                    (string-match-p
                     "\\[workspace-7\\]"
                     (substring-no-properties hidden))
                    (string-match-p
                     "feature/palette"
                     (substring-no-properties visible))
                    (string-match-p
                     "feature/palette"
                     (substring-no-properties hidden)))))
             (delete-other-windows active-window)
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ("%*  [workspace-7]  %b   utf-8-unix %3p═%l/1ln :%3c " "%*  [workspace-7]  %b   utf-8-unix %3p═%l/1ln :%3c " 0 5 5 nil nil)"#
        ]],
    )
    .fresh_process()
}

fn airline_themes_real_powerline_separator_variants_change_rendering_without_losing_content()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_real_powerline_separator_variants_change_rendering_without_losing_content",
        r##"(progn
         (load-theme 'airline-doom-one t)
         (delete-other-windows)
         (let ((buffer
                (get-buffer-create
                 "airline-separators.md")))
           (unwind-protect
               (progn
                 (set-window-buffer (selected-window) buffer)
                 (with-current-buffer buffer
                   (erase-buffer)
                   (insert "# Heading\n\nPractical content.\n")
                   (text-mode)
                   (setq buffer-file-coding-system
                         'utf-8-unix
                         powerline-selected-window
                         (selected-window))
                   (mapcar
                    (lambda (separator)
                      (setq powerline-default-separator
                            separator)
                      (powerline-reset)
                      (let ((rendered
                             (eval
                              (airline-themes-mode-line-format)
                              t)))
                        (list
                         separator
                         (substring-no-properties rendered)
                         (length rendered)
                         (secure-hash
                          'sha256
                          (prin1-to-string
                           (mapcar
                            (lambda (index)
                              (text-properties-at
                               index rendered))
                            (number-sequence
                             0
                             (1- (length rendered)))))))))
                    '(arrow utf-8 butt))))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((arrow "%*   %b   utf-8-unix %3p═%l/3ln :%3c " 42 "4c09a74ce2c07568bc49a86264bc1609787a3467160b57318d2d292aee03093d") (utf-8 "%*   %b   utf-8-unix %3p═%l/3ln :%3c " 42 "4c09a74ce2c07568bc49a86264bc1609787a3467160b57318d2d292aee03093d") (butt "%*   %b   utf-8-unix %3p═%l/3ln :%3c " 42 "4c09a74ce2c07568bc49a86264bc1609787a3467160b57318d2d292aee03093d"))"#
        ]],
    )
    .fresh_process()
}

fn airline_themes_optional_evil_states_render_full_and_narrow_real_state_labels() -> ParityBatchCase
{
    ParityBatchCase::value(
        "airline_themes_optional_evil_states_render_full_and_narrow_real_state_labels",
        r##"(progn
         (load-theme 'airline-doom-one t)
         (provide 'evil)
         (defvar evil-state 'normal)
         (defvar evil-visual-selection 'char)
         (fset 'evil-visual-state-p
               (lambda () (eq evil-state 'visual)))
         (delete-other-windows)
         (let ((buffer
                (get-buffer-create
                 "airline-evil-state.el")))
           (unwind-protect
               (progn
                 (set-window-buffer (selected-window) buffer)
                 (with-current-buffer buffer
                   (erase-buffer)
                   (insert "(setq state 'practical)\n")
                   (emacs-lisp-mode)
                   (setq buffer-file-coding-system
                         'utf-8-unix
                         powerline-selected-window
                         (selected-window))
                   (mapcar
                    (lambda (case)
                      (setq evil-state (car case)
                            evil-visual-selection
                            (cadr case))
                      (let ((rendered
                             (eval
                              (airline-themes-mode-line-format)
                              t)))
                        (list
                         case
                         (substring-no-properties rendered)
                         (get-text-property
                          0 'face rendered))))
                    '((normal char)
                      (insert char)
                      (visual char)
                      (visual line)
                      (visual block)
                      (replace char)
                      (emacs char)))))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK (((normal char) " NORMAL %*  %b   utf-8-unix %3p═%l/1ln :%3c " (airline-normal-outer)) ((insert char) " INSERT %*  %b   utf-8-unix %3p═%l/1ln :%3c " (airline-insert-outer)) ((visual char) " VISUAL %*  %b   utf-8-unix %3p═%l/1ln :%3c " (airline-visual-outer)) ((visual line) " VISUAL-LINE %*  %b   utf-8-unix %3p═%l/1ln :%3c " (airline-visual-outer)) ((visual block) " VISUAL-BLOCK %*  %b   utf-8-unix %3p═%l/1ln :%3c " (airline-visual-outer)) ((replace char) " REPLACE %*  %b   utf-8-unix %3p═%l/1ln :%3c " (airline-replace-outer)) ((emacs char) " EMACS %*  %b   utf-8-unix %3p═%l/1ln :%3c " (airline-emacs-outer)))"#
        ]],
    )
    .fresh_process()
}

fn airline_themes_directory_display_and_representative_major_modes_render_real_workflows()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_directory_display_and_representative_major_modes_render_real_workflows",
        r##"(progn
         (load-theme 'airline-catppuccin_mocha t)
         (delete-other-windows)
         (let* ((sandbox
                 (file-name-as-directory
                  (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                (root
                (expand-file-name
                 "modes/project/src/component/"
                 sandbox))
               results)
           (make-directory root t)
           (dolist (case
                    '(("source.el" emacs-lisp-mode
                       "(defun practical () 42)\n")
                      ("notes.txt" text-mode
                       "A practical note.\n")
                      ("listing" dired-mode "")
                      ("build.log" compilation-mode
                       "src/main.rs:7: warning\n")))
             (let ((buffer
                    (get-buffer-create (car case))))
               (unwind-protect
                   (progn
                     (set-window-buffer
                      (selected-window) buffer)
                     (with-current-buffer buffer
                       (erase-buffer)
                       (insert (nth 2 case))
                       (setq default-directory root
                             directory-abbrev-alist
                             (list
                              (cons
                               (concat "\\`"
                                       (regexp-quote
                                        sandbox))
                               "/fixture/"))
                             buffer-file-name
                             (unless
                                 (eq (cadr case)
                                     'dired-mode)
                               (expand-file-name
                                (car case) root))
                             buffer-file-coding-system
                             'utf-8-unix
                             powerline-selected-window
                             (selected-window))
                       (funcall (cadr case))
                       (let (full shortened none)
                         (setq airline-display-directory
                               'airline-directory-full
                               full
                               (eval
                                (airline-themes-mode-line-format)
                                t)
                               airline-display-directory
                               'airline-directory-shortened
                               airline-shortened-directory-length
                               16
                               shortened
                               (eval
                                (airline-themes-mode-line-format)
                                t)
                               airline-display-directory nil
                               none
                               (eval
                                (airline-themes-mode-line-format)
                                t))
                         (push
                          (list
                           (car case)
                           major-mode
                           (substring-no-properties full)
                           (substring-no-properties shortened)
                           (substring-no-properties none))
                          results))))
                 (kill-buffer buffer))))
           (nreverse results)))"##,
        expect![[
            r#"OK (("source.el" emacs-lisp-mode "%*  [ORACLE-SANDBOX]/modes/project/src/component/%b   utf-8-unix %3p═%l/1ln :%3c " "%*  /f/m/p/s/c/%b   utf-8-unix %3p═%l/1ln :%3c " "%*   %b   utf-8-unix %3p═%l/1ln :%3c ") ("notes.txt" text-mode "%*  [ORACLE-SANDBOX]/modes/project/src/component/%b   utf-8-unix %3p═%l/1ln :%3c " "%*  /f/m/p/s/c/%b   utf-8-unix %3p═%l/1ln :%3c " "%*   %b   utf-8-unix %3p═%l/1ln :%3c ") ("listing" dired-mode "%*   %b   utf-8-unix %3p═%l/0ln :%3c " "%*   %b   utf-8-unix %3p═%l/0ln :%3c " "%*   %b   utf-8-unix %3p═%l/0ln :%3c ") ("build.log" compilation-mode "%*  [ORACLE-SANDBOX]/modes/project/src/component/%b   utf-8-unix %3p═%l/1ln :%3c " "%*  /f/m/p/s/c/%b   utf-8-unix %3p═%l/1ln :%3c " "%*   %b   utf-8-unix %3p═%l/1ln :%3c "))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn modeline_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        airline_themes_modeline_expression_renders_a_real_editing_buffer_with_properties(),
        airline_themes_selected_and_inactive_windows_render_distinct_real_segments(),
        airline_themes_inactive_visibility_customizations_remove_state_branch_and_eyebrowse(),
        airline_themes_real_powerline_separator_variants_change_rendering_without_losing_content(),
        airline_themes_optional_evil_states_render_full_and_narrow_real_state_labels(),
        airline_themes_directory_display_and_representative_major_modes_render_real_workflows(),
    ]
}
