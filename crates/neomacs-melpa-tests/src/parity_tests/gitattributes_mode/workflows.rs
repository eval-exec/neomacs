use expect_test::expect;

use super::ParityBatchCase;

fn repository_and_global_attribute_files_activate_the_real_editing_mode() -> ParityBatchCase {
    let elisp_form = r####"
(mapcar
 (lambda (path)
   (with-temp-buffer
     (setq buffer-file-name path)
     (set-auto-mode)
     (if (eq major-mode 'gitattributes-mode)
         (list :path path
               :mode major-mode
               :name mode-name
               :text-parent (and (derived-mode-p 'text-mode) t)
               :eldoc eldoc-mode
               :field-navigation forward-sexp-function
               :comments (list comment-start comment-start-skip))
       (list :path path :mode major-mode :name mode-name))))
 '("/work/platform/.gitattributes"
   "/work/platform/services/api/.gitattributes"
   "/work/platform/.git/info/attributes"
   "/work/home/.config/git/attributes"
   "/work/platform/.attributes"))
"####;
    let expected = expect![[
        r##"OK ((:path "/work/platform/.gitattributes" :mode gitattributes-mode :name "Gitattributes" :text-parent t :eldoc t :field-navigation gitattributes-mode-forward-field :comments ("# " "#+\\s-*")) (:path "/work/platform/services/api/.gitattributes" :mode gitattributes-mode :name "Gitattributes" :text-parent t :eldoc t :field-navigation gitattributes-mode-forward-field :comments ("# " "#+\\s-*")) (:path "/work/platform/.git/info/attributes" :mode gitattributes-mode :name "Gitattributes" :text-parent t :eldoc t :field-navigation gitattributes-mode-forward-field :comments ("# " "#+\\s-*")) (:path "/work/home/.config/git/attributes" :mode gitattributes-mode :name "Gitattributes" :text-parent t :eldoc t :field-navigation gitattributes-mode-forward-field :comments ("# " "#+\\s-*")) (:path "/work/platform/.attributes" :mode fundamental-mode :name "Fundamental"))"##
    ]];
    ParityBatchCase::value(
        "repository_and_global_attribute_files_activate_the_real_editing_mode",
        elisp_form,
        expected,
    )
}

fn release_repository_rules_render_patterns_macros_states_and_values() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (gitattributes-mode)
  (insert "# Cross-platform release policy\n"
          "* text=auto\n"
          "*.rs text eol=lf diff=rust\n"
          "*.png -text diff\n"
          "docs/** export-ignore\n"
          "*.secret !filter\n"
          "[attr]binary -diff -merge -text\n"
          "*.bin binary\n"
          "data/[0-9][0-9]/*.csv filter=lfs diff=lfs merge=lfs -text\n")
  (list :mode major-mode
        :lines (line-number-at-pos (point-max))
        :faces (neomacs-gitattributes-test-face-spans)
        :text (buffer-substring-no-properties (point-min) (point-max))))
"####;
    let expected = expect![[
        r##"OK (:mode gitattributes-mode :lines 10 :faces ((:range (1 3) :text "# " :face font-lock-comment-delimiter-face) (:range (3 33) :text "Cross-platform release policy\n" :face font-lock-comment-face) (:range (33 34) :text "*" :face font-lock-keyword-face) (:range (34 35) :text " " :face font-lock-constant-face) (:range (35 39) :text "text" :face font-lock-variable-name-face) (:range (45 46) :text "*" :face font-lock-keyword-face) (:range (49 50) :text " " :face font-lock-constant-face) (:range (50 54) :text "text" :face font-lock-variable-name-face) (:range (55 58) :text "eol" :face font-lock-variable-name-face) (:range (62 66) :text "diff" :face font-lock-variable-name-face) (:range (72 73) :text "*" :face font-lock-keyword-face) (:range (77 78) :text " " :face font-lock-constant-face) (:range (78 79) :text "-" :face font-lock-negation-char-face) (:range (79 83) :text "text" :face font-lock-variable-name-face) (:range (84 88) :text "diff" :face font-lock-variable-name-face) (:range (93 94) :text "/" :face font-lock-constant-face) (:range (94 96) :text "**" :face font-lock-keyword-face) (:range (96 97) :text " " :face font-lock-constant-face) (:range (97 110) :text "export-ignore" :face font-lock-variable-name-face) (:range (111 112) :text "*" :face font-lock-keyword-face) (:range (119 120) :text " " :face font-lock-constant-face) (:range (120 121) :text "!" :face font-lock-negation-char-face) (:range (121 127) :text "filter" :face font-lock-variable-name-face) (:range (128 134) :text "[attr]" :face font-lock-function-name-face) (:range (140 141) :text " " :face font-lock-constant-face) (:range (141 142) :text "-" :face font-lock-negation-char-face) (:range (142 146) :text "diff" :face font-lock-variable-name-face) (:range (147 148) :text "-" :face font-lock-negation-char-face) (:range (148 153) :text "merge" :face font-lock-variable-name-face) (:range (154 155) :text "-" :face font-lock-negation-char-face) (:range (155 159) :text "text" :face font-lock-variable-name-face) (:range (160 161) :text "*" :face font-lock-keyword-face) (:range (165 166) :text " " :face font-lock-constant-face) (:range (166 172) :text "binary" :face font-lock-variable-name-face) (:range (177 178) :text "/" :face font-lock-constant-face) (:range (178 188) :text "[0-9][0-9]" :face font-lock-keyword-face) (:range (188 189) :text "/" :face font-lock-constant-face) (:range (189 190) :text "*" :face font-lock-keyword-face) (:range (195 201) :text "filter" :face font-lock-variable-name-face) (:range (206 210) :text "diff" :face font-lock-variable-name-face) (:range (215 220) :text "merge" :face font-lock-variable-name-face) (:range (225 226) :text "-" :face font-lock-negation-char-face) (:range (226 230) :text "text" :face font-lock-variable-name-face)) :text "# Cross-platform release policy\n* text=auto\n*.rs text eol=lf diff=rust\n*.png -text diff\ndocs/** export-ignore\n*.secret !filter\n[attr]binary -diff -merge -text\n*.bin binary\ndata/[0-9][0-9]/*.csv filter=lfs diff=lfs merge=lfs -text\n")"##
    ]];
    ParityBatchCase::value(
        "release_repository_rules_render_patterns_macros_states_and_values",
        elisp_form,
        expected,
    )
}

fn eldoc_explains_set_unset_unspecified_and_valued_attributes() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (gitattributes-mode)
  (insert "*.txt text\n"
          "*.bin -text\n"
          "*.raw !diff\n"
          "*.py diff=python\n"
          "*.dat filter=largefiles\n"
          "*.custom mystery=value\n")
  (let ((observations
         (mapcar #'neomacs-gitattributes-test-eldoc-at
                 '(" text" " -text" " !diff" " diff=python"
                   " filter=largefiles" " mystery=value"))))
    (goto-char (point-min))
    (search-forward " diff=python")
    (goto-char (- (point) (length " diff=python")))
    (list :mode major-mode
          :eldoc-mode eldoc-mode
          :provider eldoc-documentation-function
          :observations observations
          :without-state (gitattributes-mode-eldoc t))))
"####;
    let expected = expect![[
        r#"OK (:mode gitattributes-mode :eldoc-mode t :provider gitattributes-mode-eldoc :observations ((:marker " text" :line 1 :column 5 :documentation "[Set] This attribute enables and controls end-of-line normalization.") (:marker " -text" :line 2 :column 5 :documentation "[Unset] This attribute enables and controls end-of-line normalization.") (:marker " !diff" :line 3 :column 5 :documentation "[Unspecified] The attribute diff affects how Git generates diffs for particular files.") (:marker " diff=python" :line 4 :column 4 :documentation "[Set to a value] The attribute diff affects how Git generates diffs for particular files.") (:marker " filter=largefiles" :line 5 :column 5 :documentation "[Set to a value] A filter attribute can be set to a string value that names a filter driver specified in the configuration.") (:marker " mystery=value" :line 6 :column 8 :documentation nil)) :without-state "The attribute diff affects how Git generates diffs for particular files.")"#
    ]];
    ParityBatchCase::value(
        "eldoc_explains_set_unset_unspecified_and_valued_attributes",
        elisp_form,
        expected,
    )
}

fn field_commands_navigate_and_remove_one_attribute_without_damaging_neighbors() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (gitattributes-mode)
  (insert "*.md text eol=crlf diff=markdown export-subst\n")
  (let ((kill-ring nil)
        (kill-ring-yank-pointer nil)
        (select-enable-clipboard nil)
        (forward-command (key-binding (kbd "C-M-f")))
        (backward-command (key-binding (kbd "C-M-b")))
        (kill-command (key-binding (kbd "C-M-k")))
        positions)
    (goto-char (point-min))
    (push (neomacs-gitattributes-test-position) positions)
    (call-interactively forward-command)
    (push (neomacs-gitattributes-test-position) positions)
    (call-interactively forward-command)
    (push (neomacs-gitattributes-test-position) positions)
    (call-interactively kill-command)
    (push (neomacs-gitattributes-test-position) positions)
    (call-interactively forward-command)
    (push (neomacs-gitattributes-test-position) positions)
    (call-interactively backward-command)
    (push (neomacs-gitattributes-test-position) positions)
    (list :commands (list forward-command backward-command kill-command)
          :positions (nreverse positions)
          :killed (current-kill 0 t)
          :buffer (buffer-substring-no-properties (point-min) (point-max))
          :modified (buffer-modified-p))))
"####;
    let expected = expect![[
        r#"OK (:commands (forward-sexp backward-sexp kill-sexp) :positions ((:point 1 :column 0 :field "*") (:point 6 :column 5 :field "text") (:point 11 :column 10 :field "eol") (:point 11 :column 10 :field "diff") (:point 25 :column 24 :field "export-subst") (:point 10 :column 9 :field "text")) :killed "eol=crlf " :buffer "*.md text diff=markdown export-subst\n" :modified t)"#
    ]];
    ParityBatchCase::value(
        "field_commands_navigate_and_remove_one_attribute_without_damaging_neighbors",
        elisp_form,
        expected,
    )
}

fn commenting_selected_release_rules_round_trips_text_and_rendering() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (gitattributes-mode)
  (insert "*.rs text eol=lf\n"
          "*.png -text\n"
          "archives/** export-ignore\n"
          "*.pdf diff\n")
  (let ((transient-mark-mode t)
        (comment-command (key-binding (kbd "M-;"))))
    (goto-char (point-min))
    (forward-line 1)
    (let ((start (point)))
      (forward-line 2)
      (push-mark start t t)
      (call-interactively comment-command))
    (let ((commented (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (forward-line 1)
      (let ((start (point)))
        (forward-line 2)
        (push-mark start t t)
        (call-interactively comment-command))
      (font-lock-flush)
      (list :command comment-command
            :commented commented
            :restored (buffer-substring-no-properties (point-min) (point-max))
            :faces (neomacs-gitattributes-test-face-spans)
            :comment-vars (list comment-start comment-start-skip comment-end)))))
"####;
    let expected = expect![[
        r##"OK (:command comment-dwim :commented "*.rs text eol=lf\n# *.png -text\n# archives/** export-ignore\n*.pdf diff\n" :restored "*.rs text eol=lf\n*.png -text\narchives/** export-ignore\n*.pdf diff\n" :faces ((:range (1 2) :text "*" :face font-lock-keyword-face) (:range (5 6) :text " " :face font-lock-constant-face) (:range (6 10) :text "text" :face font-lock-variable-name-face) (:range (11 14) :text "eol" :face font-lock-variable-name-face) (:range (18 19) :text "*" :face font-lock-keyword-face) (:range (23 24) :text " " :face font-lock-constant-face) (:range (24 25) :text "-" :face font-lock-negation-char-face) (:range (25 29) :text "text" :face font-lock-variable-name-face) (:range (38 39) :text "/" :face font-lock-constant-face) (:range (39 41) :text "**" :face font-lock-keyword-face) (:range (41 42) :text " " :face font-lock-constant-face) (:range (42 55) :text "export-ignore" :face font-lock-variable-name-face) (:range (56 57) :text "*" :face font-lock-keyword-face) (:range (62 66) :text "diff" :face font-lock-variable-name-face)) :comment-vars ("# " "#+\\s-*" ""))"##
    ]];
    ParityBatchCase::value(
        "commenting_selected_release_rules_round_trips_text_and_rendering",
        elisp_form,
        expected,
    )
}

fn help_command_routes_the_exact_manual_topic_to_the_configured_viewer() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((calls nil)
       (help-buffer nil)
       (gitattributes-mode-man-function
        (lambda (topic)
          (push topic calls)
          (setq help-buffer (get-buffer-create "*gitattributes-manual*")))))
  (unwind-protect
      (with-temp-buffer
        (gitattributes-mode)
        (let ((origin (current-buffer))
              (result (gitattributes-mode-help)))
          (list :calls (nreverse calls)
                :returned-help-buffer (eq result help-buffer)
                :help-buffer (buffer-name help-buffer)
                :origin-preserved (eq origin (current-buffer))
                :mode major-mode)))
    (when (buffer-live-p help-buffer)
      (kill-buffer help-buffer))))
"####;
    let expected = expect![[
        r#"OK (:calls ("gitattributes") :returned-help-buffer t :help-buffer "*gitattributes-manual*" :origin-preserved t :mode gitattributes-mode)"#
    ]];
    ParityBatchCase::value(
        "help_command_routes_the_exact_manual_topic_to_the_configured_viewer",
        elisp_form,
        expected,
    )
}

fn help_command_preserves_a_missing_manual_viewer_failure() -> ParityBatchCase {
    let elisp_form = r####"
(let ((gitattributes-mode-man-function
       (lambda (topic)
         (signal 'file-missing
                 (list "Searching for program"
                       "No such file or directory"
                       topic)))))
  (gitattributes-mode-help))
"####;
    let expected = expect![[
        r#"ERR (file-missing "Searching for program" "No such file or directory" "gitattributes")"#
    ]];
    ParityBatchCase::signal(
        "help_command_preserves_a_missing_manual_viewer_failure",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        repository_and_global_attribute_files_activate_the_real_editing_mode(),
        release_repository_rules_render_patterns_macros_states_and_values(),
        eldoc_explains_set_unset_unspecified_and_valued_attributes(),
        field_commands_navigate_and_remove_one_attribute_without_damaging_neighbors(),
        commenting_selected_release_rules_round_trips_text_and_rendering(),
        help_command_routes_the_exact_manual_topic_to_the_configured_viewer(),
        help_command_preserves_a_missing_manual_viewer_failure(),
    ]
}
