use expect_test::expect;

use super::ParityBatchCase;

pub(super) fn default_load_case() -> ParityBatchCase {
    let elisp_form = "t";
    let expected = expect!["ERR (void-variable flymake-allowed-file-name-masks)"];
    ParityBatchCase::signal(
        "default_load_reports_the_missing_legacy_flymake_dependency",
        elisp_form,
        expected,
    )
    .setup_outcome()
}

fn visiting_a_project_stylesheet_activates_the_complete_editing_surface() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-scss-test-with-isolated-globals
 (let* ((root (neomacs-scss-test-root "scss-visit"))
       (file (expand-file-name "assets/styles/app.scss" root))
       buffer)
  (unwind-protect
      (progn
        (make-directory (file-name-directory file) t)
        (with-temp-file file
          (insert "$brand: #336699;\n.card { color: $brand; }\n"))
        (setq buffer (find-file-noselect file))
        (with-current-buffer buffer
          (list
           :mode (list major-mode mode-name
                       (derived-mode-p 'css-mode)
                       (derived-mode-p 'prog-mode))
           :file (file-relative-name buffer-file-name root)
           :compile-key (key-binding (kbd "C-c C-c"))
           :comments (list comment-start comment-start-skip comment-end)
           :after-save (and (memq 'scss-compile-maybe after-save-hook) t)
           :compile-regexp
           (and (member scss-compile-error-regex
                        compilation-error-regexp-alist)
                t)
           :auto-mode (cdr (assoc "\\.scss\\'" auto-mode-alist)))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (when (file-exists-p root) (delete-directory root t)))))
"##;
    let expected = expect![[
        r#"OK (:mode (scss-mode "SCSS" css-mode prog-mode) :file "assets/styles/app.scss" :compile-key scss-compile :comments ("/*" "/\\*+[ \11]*" "*/") :after-save t :compile-regexp t :auto-mode scss-mode)"#
    ]];
    ParityBatchCase::value(
        "visiting_a_project_stylesheet_activates_the_complete_editing_surface",
        elisp_form,
        expected,
    )
}

fn authoring_nested_components_indents_variables_mixins_and_media_queries() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-scss-test-with-isolated-globals
 (with-temp-buffer
  (scss-mode)
  (setq-local indent-tabs-mode nil)
  (setq-local css-indent-offset 2)
  (insert
   "$brand: #336699;\n"
   "$breakpoint: 48rem;\n\n"
   "@mixin button-theme($tone) {\n"
   "color: $tone;\n"
   "&:hover {\n"
   "background: darken($tone, 10%);\n"
   "}\n"
   "}\n\n"
   ".toolbar {\n"
   "display: flex;\n"
   ".button {\n"
   "@include button-theme($brand);\n"
   "span {\n"
   "font-weight: 600;\n"
   "}\n"
   "}\n"
   "@media (min-width: $breakpoint) {\n"
   "gap: 1.5rem;\n"
   "}\n"
   "}\n")
  (indent-region (point-min) (point-max))
  (list :indent-function indent-line-function
        :text (buffer-substring-no-properties (point-min) (point-max)))))
"##;
    let expected = expect![[
        r#"OK (:indent-function smie-indent-line :text "$brand: #336699;\n$breakpoint: 48rem;\n\n@mixin button-theme($tone) {\n  color: $tone;\n  &:hover {\n    background: darken($tone, 10%);\n  }\n}\n\n.toolbar {\n  display: flex;\n  .button {\n    @include button-theme($brand);\n    span {\n      font-weight: 600;\n    }\n  }\n  @media (min-width: $breakpoint) {\n    gap: 1.5rem;\n  }\n}\n")"#
    ]];
    ParityBatchCase::value(
        "authoring_nested_components_indents_variables_mixins_and_media_queries",
        elisp_form,
        expected,
    )
}

fn fontification_and_syntax_distinguish_variables_strings_and_both_comment_styles()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-scss-test-with-isolated-globals
 (with-temp-buffer
  (scss-mode)
  (insert
   "$brand-tone: #336699;\n"
   "$message: \"deploy // literally\";\n"
   "// operator note with $ignored\n"
   "/* release fallback */\n"
   ".card[data-state=\"ready\"] {\n"
   "  color: $brand-tone;\n"
   "}\n")
  (font-lock-ensure (point-min) (point-max))
  (list
   :faces (mapcar #'neomacs-scss-test-face-at
                  '("$brand-tone" ".card" "color" "$brand-tone;"))
   :syntax (mapcar #'neomacs-scss-test-syntax-at
                   '("deploy // literally" "operator note"
                     "release fallback" "ready")))))
"##;
    let expected = expect![[
        r#"OK (:faces (("$brand-tone" font-lock-constant-face) (".card" css-selector) ("color" css-property) ("$brand-tone;" font-lock-constant-face)) :syntax (("deploy // literally" :string 34 :comment nil) ("operator note" :string nil :comment t) ("release fallback" :string nil :comment t) ("ready" :string 34 :comment nil)))"#
    ]];
    ParityBatchCase::value(
        "fontification_and_syntax_distinguish_variables_strings_and_both_comment_styles",
        elisp_form,
        expected,
    )
}

fn compile_command_preserves_project_directory_options_and_output_destination() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-scss-test-with-isolated-globals
 (let* ((root (neomacs-scss-test-root "scss-compile"))
       (source-dir (expand-file-name "assets/theme source" root))
       (file (expand-file-name "main.scss" source-dir))
       (scss-sass-command "sass")
       (scss-sass-options '("--style" "compressed" "--sourcemap=none"))
       (scss-output-directory "../../public/theme css")
       command)
  (unwind-protect
      (progn
        (make-directory source-dir t)
        (with-temp-file file (insert "$brand: #369;\n"))
        (with-temp-buffer
          (setq buffer-file-name file)
          (scss-mode)
          (cl-letf (((symbol-function 'compile)
                     (lambda (value &optional _comint)
                       (setq command value)
                       :planned)))
            (list :result (scss-compile)
                  :command (neomacs-scss-test-normalize command)))))
    (when (file-exists-p root) (delete-directory root t)))))
"##;
    let expected = expect![[
        r#"OK (:result :planned :command "sass --style compressed --sourcemap=none --update '<ROOT>/scss-compile/assets/theme source/':'../../public/theme css'")"#
    ]];
    ParityBatchCase::value(
        "compile_command_preserves_project_directory_options_and_output_destination",
        elisp_form,
        expected,
    )
}

fn saving_a_real_stylesheet_compiles_only_while_compile_at_save_is_enabled() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-scss-test-with-isolated-globals
 (let* ((root (neomacs-scss-test-root "scss-save"))
       (file (expand-file-name "styles/dashboard.scss" root))
       (scss-sass-command "sass")
       (scss-sass-options '("--style" "expanded"))
       (scss-output-directory "../css")
       buffer calls)
  (unwind-protect
      (progn
        (make-directory (file-name-directory file) t)
        (with-temp-file file (insert "$gap: 8px;\n"))
        (setq buffer (find-file-noselect file))
        (with-current-buffer buffer
          (setq-local scss-compile-at-save t)
          (goto-char (point-max))
          (insert ".dashboard { gap: $gap; }\n")
          (cl-letf (((symbol-function 'compile)
                     (lambda (command &optional _comint)
                       (push (neomacs-scss-test-normalize command) calls)
                       :planned)))
            (save-buffer))
          (setq-local scss-compile-at-save nil)
          (goto-char (point-max))
          (insert ".dashboard--dense { gap: 4px; }\n")
          (cl-letf (((symbol-function 'compile)
                     (lambda (command &optional _comint)
                       (push (neomacs-scss-test-normalize command) calls)
                       :unexpected)))
            (save-buffer))
          (list :calls (nreverse calls)
                :disk (with-temp-buffer
                        (insert-file-contents file)
                        (buffer-string)))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (when (file-exists-p root) (delete-directory root t)))))
"##;
    let expected = expect![[
        r#"OK (:calls ("sass --style expanded --update '<ROOT>/scss-save/styles/':'../css'") :disk "$gap: 8px;\n.dashboard { gap: $gap; }\n.dashboard--dense { gap: 4px; }\n")"#
    ]];
    ParityBatchCase::value(
        "saving_a_real_stylesheet_compiles_only_while_compile_at_save_is_enabled",
        elisp_form,
        expected,
    )
}

fn enabling_flymake_runs_the_registered_scss_checker_and_reports_a_diagnostic() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-scss-test-with-isolated-globals
 (let* ((root (neomacs-scss-test-root "scss-flymake"))
        (file (expand-file-name "styles/account.scss" root))
        (checker (expand-file-name "bin/sass-check" root))
        (argv-log (expand-file-name "checker.argv" root))
        (scss-sass-command checker)
        (scss-sass-options (list argv-log "--style" "expanded"))
        (flymake-start-on-flymake-mode nil)
        buffer)
   (unwind-protect
       (progn
         (make-directory (file-name-directory file) t)
         (make-directory (file-name-directory checker) t)
         (neomacs-scss-test-write-checker checker nil)
         (with-temp-file file
           (insert "$tone: #246;\n.account { color: ; }\n"))
         (setq buffer (find-file-noselect file))
         (with-current-buffer buffer
           (flymake-mode 1)
           (flymake-start nil t)
           (let ((deadline (+ (float-time) 10)))
             (while (and (< (float-time) deadline)
                         (not (flymake-diagnostics
                               (point-min) (point-max)))
                         (not (flymake-disabled-backends)))
               (accept-process-output nil 0.05)))
           (let ((diagnostics (flymake-diagnostics (point-min) (point-max))))
             (list
             :mode flymake-mode
              :registered
              (and (member '(".+\\.scss$" flymake-scss-init)
                           flymake-allowed-file-name-masks)
                   t)
              :disabled (flymake-disabled-backends)
              :argv
              (with-temp-buffer
                (insert-file-contents argv-log)
                (replace-regexp-in-string
                 "_[0-9]+_flymake\\.scss" "_<ID>_flymake.scss"
                 (buffer-string)))
              :diagnostics
              (mapcar (lambda (diagnostic)
                        (list (flymake-diagnostic-type diagnostic)
                              (line-number-at-pos
                               (flymake-diagnostic-beg diagnostic))
                              (replace-regexp-in-string
                               "_[0-9]+_flymake\\.scss"
                               "_<ID>_flymake.scss"
                               (flymake-diagnostic-text diagnostic))))
                      diagnostics)))))
     (when (buffer-live-p buffer)
       (with-current-buffer buffer
         (when flymake-mode (flymake-mode -1))
         (set-buffer-modified-p nil))
       (kill-buffer buffer))
     (when (file-exists-p root) (delete-directory root t)))))
"##;
    let expected = expect![[
        r#"OK (:mode t :registered t :disabled nil :argv "--style\nexpanded\n--scss\n--check\naccount_<ID>_flymake.scss\n" :diagnostics ((:error 2 "account_<ID>_flymake.scss")))"#
    ]];
    ParityBatchCase::value(
        "enabling_flymake_runs_the_registered_scss_checker_and_reports_a_diagnostic",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn newline_terminated_sass_output_exposes_the_legacy_flymake_parser_failure() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-scss-test-with-isolated-globals
 (let* ((root (neomacs-scss-test-root "scss-flymake-newline"))
        (file (expand-file-name "styles/account.scss" root))
        (checker (expand-file-name "bin/sass-check" root))
        (argv-log (expand-file-name "checker.argv" root))
        (scss-sass-command checker)
        (scss-sass-options (list argv-log "--style" "expanded"))
        (flymake-start-on-flymake-mode nil)
        buffer)
   (unwind-protect
       (progn
         (make-directory (file-name-directory file) t)
         (make-directory (file-name-directory checker) t)
         (neomacs-scss-test-write-checker checker t)
         (with-temp-file file
           (insert "$tone: #246;\n.account { color: ; }\n"))
         (setq buffer (find-file-noselect file))
         (with-current-buffer buffer
           (flymake-mode 1)
           (flymake-start nil t)
           (let ((deadline (+ (float-time) 10)))
             (while (and (< (float-time) deadline)
                         (not (flymake-diagnostics
                               (point-min) (point-max)))
                         (not (flymake-disabled-backends)))
               (accept-process-output nil 0.05)))
           (list
            :mode flymake-mode
            :registered
            (and (member '(".+\\.scss$" flymake-scss-init)
                         flymake-allowed-file-name-masks)
                 t)
            :argv
            (with-temp-buffer
              (insert-file-contents argv-log)
              (replace-regexp-in-string
               "_[0-9]+_flymake\\.scss" "_<ID>_flymake.scss"
               (buffer-string)))
            :disabled (flymake-disabled-backends)
            :diagnostics
            (mapcar (lambda (diagnostic)
                      (list (flymake-diagnostic-type diagnostic)
                            (line-number-at-pos
                             (flymake-diagnostic-beg diagnostic))
                            (flymake-diagnostic-text diagnostic)))
                    (flymake-diagnostics (point-min) (point-max))))))
     (when (buffer-live-p buffer)
       (with-current-buffer buffer
         (when flymake-mode (flymake-mode -1))
         (set-buffer-modified-p nil))
       (kill-buffer buffer))
     (when (file-exists-p root) (delete-directory root t)))))
"##;
    let expected = expect![[
        r#"OK (:mode t :registered t :argv "--style\nexpanded\n--scss\n--check\naccount_<ID>_flymake.scss\n" :disabled (flymake-proc-legacy-flymake) :diagnostics nil)"#
    ]];
    ParityBatchCase::value(
        "newline_terminated_sass_output_exposes_the_legacy_flymake_parser_failure",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn compilation_diagnostics_navigate_to_the_exact_source_line() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-scss-test-with-isolated-globals
 (let* ((root (neomacs-scss-test-root "scss-diagnostics"))
       (file (expand-file-name "styles/broken.scss" root))
       (source nil)
       (errors (generate-new-buffer "*scss-diagnostic-output*")))
  (unwind-protect
      (save-window-excursion
        (make-directory (file-name-directory file) t)
        (with-temp-file file
          (insert "$brand: #369;\n.card {\n  color: ;\n}\n"))
        (setq source (find-file-noselect file))
        (with-current-buffer source (scss-mode))
        (switch-to-buffer errors)
        (insert "sass --update styles/\n")
        (insert "Syntax error: Invalid CSS after `color:'\n")
        (insert (format "        on line 3 of %s\n" file))
        (compilation-mode)
        (setq-local compilation-error-regexp-alist
                    (list scss-compile-error-regex))
        (goto-char (point-min))
        (compilation-next-error 1)
        (compile-goto-error)
        (list :file (neomacs-scss-test-normalize buffer-file-name)
              :line (line-number-at-pos)
              :column (current-column)
              :source (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position))))
    (when (buffer-live-p errors) (kill-buffer errors))
    (when (buffer-live-p source)
      (with-current-buffer source (set-buffer-modified-p nil))
      (kill-buffer source))
    (when (file-exists-p root) (delete-directory root t)))))
"##;
    let expected = expect![[
        r#"OK (:file "<ROOT>/scss-diagnostics/styles/broken.scss" :line 3 :column 2 :source "  color: ;")"#
    ]];
    ParityBatchCase::value(
        "compilation_diagnostics_navigate_to_the_exact_source_line",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        visiting_a_project_stylesheet_activates_the_complete_editing_surface(),
        authoring_nested_components_indents_variables_mixins_and_media_queries(),
        fontification_and_syntax_distinguish_variables_strings_and_both_comment_styles(),
        compile_command_preserves_project_directory_options_and_output_destination(),
        saving_a_real_stylesheet_compiles_only_while_compile_at_save_is_enabled(),
        enabling_flymake_runs_the_registered_scss_checker_and_reports_a_diagnostic(),
        newline_terminated_sass_output_exposes_the_legacy_flymake_parser_failure(),
        compilation_diagnostics_navigate_to_the_exact_source_line(),
    ]
}
