use expect_test::expect;

use super::ParityBatchCase;

fn sass_file_activation_configures_the_real_editing_contract() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((sass-indent-offset 4)
        (buffer-file-name "/workspace/assets/application.sass"))
    (set-auto-mode)
    (list :mode major-mode
          :name mode-name
          :derived-from-haml (derived-mode-p 'haml-mode)
          :indent-line indent-line-function
          :indent-region indent-region-function
          :indent-p haml-indent-function
          :offset haml-indent-offset
          :tabs indent-tabs-mode
          :electric-indent electric-indent-inhibit
          :comment (list comment-start comment-start-skip)
          :syntax (list (char-syntax ?-) (char-syntax ?_))
          :keys (list (key-binding (kbd "C-c C-r"))
                      (key-binding (kbd "C-c C-l"))
                      (key-binding (kbd "C-c C-f")))
          :font-lock font-lock-defaults)))
"##;
    let expected = expect![[
        r#"OK (:mode sass-mode :name "Sass" :derived-from-haml haml-mode :indent-line haml-indent-line :indent-region haml-indent-region :indent-p sass-indent-p :offset 4 :tabs nil :electric-indent t :comment ("/*" "/[/*] *") :syntax (119 119) :keys (sass-output-region sass-output-buffer haml-forward-sexp) :font-lock (sass-font-lock-keywords t t))"#
    ]];
    ParityBatchCase::value(
        "sass_file_activation_configures_the_real_editing_contract",
        elisp_form,
        expected,
    )
}

fn line_by_line_authoring_indents_a_nested_responsive_component() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (sass-mode)
  (insert "$brand: #c00\n"
          ".release-card\n"
          "@media screen and (min-width: 40rem)\n"
          "&:hover\n"
          "color: $brand\n")
  (goto-char (point-min))
  (forward-line 1)
  (while (not (eobp))
    (indent-according-to-mode)
    (forward-line 1))
  (goto-char (point-min))
  (let (lines)
    (while (not (eobp))
      (push (list :indent (current-indentation)
                  :can-nest (save-excursion
                              (back-to-indentation)
                              (sass-indent-p))
                  :text (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position)))
            lines)
      (forward-line 1))
    (list :source (buffer-string)
          :lines (nreverse lines))))
"##;
    let expected = expect![[
        r#"OK (:source "$brand: #c00\n.release-card\n  @media screen and (min-width: 40rem)\n    &:hover\n      color: $brand\n" :lines ((:indent 0 :can-nest nil :text "$brand: #c00") (:indent 0 :can-nest t :text ".release-card") (:indent 2 :can-nest t :text "  @media screen and (min-width: 40rem)") (:indent 4 :can-nest t :text "    &:hover") (:indent 6 :can-nest t :text "      color: $brand")))"#
    ]];
    ParityBatchCase::value(
        "line_by_line_authoring_indents_a_nested_responsive_component",
        elisp_form,
        expected,
    )
}

fn font_lock_distinguishes_selectors_variables_directives_colors_and_comments() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (sass-mode)
  (insert "!brand = #c00\n"
          ".release-card:hover\n"
          "  color: !brand\n"
          "  @if !enabled and not !paused\n"
          "    background: blue\n"
          "  @else if !fallback\n"
          "    background: black\n"
          "  /* deployment note */\n")
  (font-lock-ensure (point-min) (point-max))
  (list :faces (neomacs-sass-mode-test-face-runs)))
"##;
    let expected = expect![[
        r##"OK (:faces (("!brand" font-lock-variable-name-face) ("#c00" font-lock-preprocessor-face) (".release-card" font-lock-type-face) (":hover" font-lock-function-name-face) ("color:" font-lock-variable-name-face) ("@if" font-lock-keyword-face) ("!enabled" font-lock-variable-name-face) ("and" font-lock-keyword-face) ("not" font-lock-keyword-face) ("!paused" font-lock-variable-name-face) ("background:" font-lock-variable-name-face) ("@else if" font-lock-keyword-face) ("!fallback" font-lock-variable-name-face) ("background:" font-lock-variable-name-face) ("/* deployment note */" font-lock-comment-face)))"##
    ]];
    ParityBatchCase::value(
        "font_lock_distinguishes_selectors_variables_directives_colors_and_comments",
        elisp_form,
        expected,
    )
}

fn inherited_block_navigation_and_backspace_promote_a_nested_status_selector() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (sass-mode)
  (insert ".release\n"
          "  @media screen\n"
          "    &:hover\n"
          "      color: red\n"
          "  .status\n"
          "    color: green\n")
  (goto-char (point-min))
  (let (media hover up sibling)
    (haml-down-list 1)
    (setq media (list (point) (current-indentation)
                      (buffer-substring-no-properties
                       (point) (line-end-position))))
    (haml-down-list 1)
    (setq hover (list (point) (current-indentation)
                      (buffer-substring-no-properties
                       (point) (line-end-position))))
    (haml-up-list 1)
    (setq up (list (point) (current-indentation)
                   (buffer-substring-no-properties
                    (point) (line-end-position))))
    (haml-forward-sexp 1)
    (setq sibling (list (point) (current-indentation)
                        (buffer-substring-no-properties
                         (point) (line-end-position))))
    (haml-electric-backspace 1)
    (list :media media
          :hover hover
          :up up
          :sibling sibling
          :promoted (buffer-string)
          :point (point)
          :column (current-column))))
"##;
    let expected = expect![[
        r#"OK (:media (12 2 "@media screen") :hover (30 4 "&:hover") :up (12 2 "@media screen") :sibling (57 2 ".status") :promoted ".release\n  @media screen\n    &:hover\n      color: red\n.status\n  color: green\n" :point 55 :column 0)"#
    ]];
    ParityBatchCase::value(
        "inherited_block_navigation_and_backspace_promote_a_nested_status_selector",
        elisp_form,
        expected,
    )
}

fn successful_region_compilation_dedents_input_and_displays_css_output() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (delete-other-windows)
  (let* ((root (expand-file-name
                "sass-success/"
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (bin-directory (expand-file-name "bin/" root))
         (compiler (expand-file-name "sass" bin-directory))
         (arguments-file (expand-file-name "arguments" root))
         (input-file (expand-file-name "input.sass" root))
         (process-environment (copy-sequence process-environment))
         (exec-path (cons bin-directory exec-path)))
    (make-directory bin-directory t)
    (with-temp-file compiler
      (insert "#!/bin/sh\n"
              "set -eu\n"
              "printf '%s\\n' \"$@\" > \"$SASS_TEST_ARGUMENTS\"\n"
              "cat > \"$SASS_TEST_INPUT\"\n"
              "printf '%s\\n' '.release {' '  color: red;' '}' "
              "'.status {' '  color: green;' '}'\n"))
    (set-file-modes compiler #o755)
    (setenv "PATH" (concat bin-directory path-separator (getenv "PATH")))
    (setenv "SASS_TEST_ARGUMENTS" arguments-file)
    (setenv "SASS_TEST_INPUT" input-file)
    (with-temp-buffer
      (sass-mode)
      (insert ".dashboard\n"
              "    .release\n"
              "      color: red\n"
              "    .status\n"
              "      color: green\n"
              ".footer\n")
      (goto-char (point-min))
      (search-forward ".release")
      (let ((source-buffer (current-buffer))
            (start (line-beginning-position))
            (end (progn (forward-line 4) (point)))
            (sass-command-options
             '("--style" "expanded" "--load-path" "vendor/styles"))
            (sass-before-eval-hook
             (list (lambda ()
                     (goto-char (point-max))
                     (insert "    // environment: production\n")))))
        (unwind-protect
            (progn
              (sass-output-region start end)
              (list
               :arguments
               (with-temp-buffer
                 (insert-file-contents arguments-file)
                 (split-string (buffer-string) "\n" t))
               :input
               (with-temp-buffer
                 (insert-file-contents input-file)
                 (buffer-string))
               :source
               (with-current-buffer source-buffer (buffer-string))
               :output (buffer-string)
               :mode major-mode
               :errors-created (and (get-buffer "*sass-errors*") t)
               :windows (length (window-list))
               :displayed (eq (window-buffer (selected-window))
                              (get-buffer "*sass-output*"))))
          (dolist (name '("*sass-output*" "*sass-errors*"))
            (when-let ((buffer (get-buffer name)))
              (kill-buffer buffer))))))))
"##;
    let expected = expect![[
        r#"OK (:arguments ("--style" "expanded" "--load-path" "vendor/styles" "--stdin") :input ".release\n  color: red\n.status\n  color: green\n\n// environment: production\n" :source ".dashboard\n    .release\n      color: red\n    .status\n      color: green\n.footer\n" :output ".release {\n  color: red;\n}\n.status {\n  color: green;\n}\n" :mode css-mode :errors-created nil :windows 2 :displayed t)"#
    ]];
    ParityBatchCase::value(
        "successful_region_compilation_dedents_input_and_displays_css_output",
        elisp_form,
        expected,
    )
}

fn failed_buffer_compilation_preserves_source_and_displays_read_only_diagnostics() -> ParityBatchCase
{
    let elisp_form = r##"
(save-window-excursion
  (delete-other-windows)
  (let* ((root (expand-file-name
                "sass-failure/"
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (bin-directory (expand-file-name "bin/" root))
         (compiler (expand-file-name "sass" bin-directory))
         (arguments-file (expand-file-name "arguments" root))
         (input-file (expand-file-name "input.sass" root))
         (process-environment (copy-sequence process-environment))
         (exec-path (cons bin-directory exec-path)))
    (make-directory bin-directory t)
    (with-temp-file compiler
      (insert "#!/bin/sh\n"
              "set -eu\n"
              "printf '%s\\n' \"$@\" > \"$SASS_TEST_ARGUMENTS\"\n"
              "cat > \"$SASS_TEST_INPUT\"\n"
              "printf '%s\\n' "
              "'Error: Invalid CSS after \"color\": expected \":\", was \"red\"' "
              "'  on line 2 of standard input' >&2\n"
              "exit 65\n"))
    (set-file-modes compiler #o755)
    (setenv "PATH" (concat bin-directory path-separator (getenv "PATH")))
    (setenv "SASS_TEST_ARGUMENTS" arguments-file)
    (setenv "SASS_TEST_INPUT" input-file)
    (with-temp-buffer
      (sass-mode)
      (insert ".release\n  color red\n")
      (let ((source-buffer (current-buffer)))
        (unwind-protect
            (progn
              (sass-output-buffer)
              (let ((errors (get-buffer "*sass-errors*"))
                    (output (get-buffer "*sass-output*")))
                (list
                 :arguments
                 (with-temp-buffer
                   (insert-file-contents arguments-file)
                   (split-string (buffer-string) "\n" t))
                 :input
                 (with-temp-buffer
                   (insert-file-contents input-file)
                   (buffer-string))
                 :source (with-current-buffer source-buffer (buffer-string))
                 :source-current (eq (current-buffer) source-buffer)
                 :output
                 (and output
                      (with-current-buffer output
                        (list :text (buffer-string)
                              :major-mode major-mode)))
                 :diagnostics
                 (with-current-buffer errors
                   (list :text (buffer-string)
                         :major-mode major-mode
                         :view-mode view-mode
                         :read-only buffer-read-only))
                 :diagnostics-displayed
                 (and (get-buffer-window errors) t)
                 :selected-diagnostics
                 (eq (window-buffer (selected-window)) errors))))
          (dolist (name '("*sass-output*" "*sass-errors*"))
            (when-let ((buffer (get-buffer name)))
              (kill-buffer buffer))))))))
"##;
    let expected = expect![[
        r#"OK (:arguments ("--stdin") :input ".release\n  color red\n\n" :source ".release\n  color red\n" :source-current t :output (:text "" :major-mode fundamental-mode) :diagnostics (:text "Error: Invalid CSS after \"color\": expected \":\", was \"red\"\n  on line 2 of standard input\n" :major-mode fundamental-mode :view-mode t :read-only t) :diagnostics-displayed t :selected-diagnostics nil)"#
    ]];
    ParityBatchCase::value(
        "failed_buffer_compilation_preserves_source_and_displays_read_only_diagnostics",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        sass_file_activation_configures_the_real_editing_contract(),
        line_by_line_authoring_indents_a_nested_responsive_component(),
        font_lock_distinguishes_selectors_variables_directives_colors_and_comments(),
        inherited_block_navigation_and_backspace_promote_a_nested_status_selector(),
        successful_region_compilation_dedents_input_and_displays_css_output(),
        failed_buffer_compilation_preserves_source_and_displays_read_only_diagnostics(),
    ]
}
