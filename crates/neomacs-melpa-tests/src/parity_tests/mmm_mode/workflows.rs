use expect_test::expect;

use super::ParityBatchCase;

fn erb_class_parses_multiple_ruby_regions_with_delimiters_faces_and_font_lock() -> ParityBatchCase {
    ParityBatchCase::value(
        "erb_class_parses_multiple_ruby_regions_with_delimiters_faces_and_font_lock",
        r####"
(neomacs-mmm-test-with-buffer
  (insert "<h1><%= title %></h1>\n<% if ready %>\n<div class=\"ok\">Ship</div>\n<% end %>\n")
  (html-erb-mode)
  (let ((mmm-mode-ext-classes-alist nil)
        (mmm-classes '(erb))
        (mmm-parse-when-idle nil)
        (mmm-submode-decoration-level 2))
    (mmm-mode-on)
    (font-lock-ensure)
    (list :state (neomacs-mmm-test-state)
          :tokens (neomacs-mmm-test-token-state
                   '("title" "if ready" "class" "\"ok\"" "end")))))
"####,
        expect![[
            r#"OK (:state (:mode t :major html-erb-mode :primary html-erb-mode :current nil :mode-name "ERB-HTML" :indent mmm-erb-indent-line :fontifier mmm-fontify-region :syntax mmm-syntax-propertize-function :overlays ((:range (8 15) :text " title " :mode ruby-mode :name nil :face mmm-output-submode-face :delimiter nil :special t) (:range (25 35) :text " if ready " :mode ruby-mode :name nil :face mmm-code-submode-face :delimiter nil :special t) (:range (67 72) :text " end " :mode ruby-mode :name nil :face mmm-code-submode-face :delimiter nil :special t)) :bindings (("C-c % c") ("C-c % x") ("C-c % r") ("C-c % b") ("C-c % k") ("C-c % z"))) :tokens (("title" 9 mmm-output-submode-face ruby-mode) ("if ready" 26 mmm-code-submode-face ruby-mode) ("class" 43 nil nil) ("\"ok\"" 49 nil nil) ("end" 68 mmm-code-submode-face ruby-mode)))"#
        ]],
    )
}

fn point_transitions_switch_submode_keymap_syntax_and_mode_line_then_restore_primary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "point_transitions_switch_submode_keymap_syntax_and_mode_line_then_restore_primary",
        r####"
(neomacs-mmm-test-with-buffer
  (insert "<p>before</p><%= user.name %><p>after</p>")
  (html-erb-mode)
  (let ((mmm-mode-ext-classes-alist nil)
        (mmm-classes '(erb))
        (mmm-parse-when-idle nil))
    (mmm-mode-on)
    (goto-char (point-min))
    (search-forward "user")
    (mmm-update-current-submode)
    (let ((inside
           (list :submode mmm-current-submode
                 :overlay (and mmm-current-overlay
                               (buffer-substring-no-properties
                                (overlay-start mmm-current-overlay)
                                (overlay-end mmm-current-overlay)))
                 :mode-name mode-name
                 :syntax-word (char-syntax ?_)
                 :comment-start comment-start
                 :indent indent-line-function)))
      (goto-char (point-max))
      (mmm-update-current-submode)
      (list :inside inside
            :outside (list :submode mmm-current-submode
                           :overlay mmm-current-overlay
                           :mode-name mode-name
                           :syntax-word (char-syntax ?_)
                           :comment-start comment-start
                           :indent indent-line-function)))))
"####,
        expect![[
            r#"OK (:inside (:submode ruby-mode :overlay " user.name " :mode-name "ERB-HTML" :syntax-word 95 :comment-start "<!-- " :indent mmm-erb-indent-line) :outside (:submode nil :overlay nil :mode-name "ERB-HTML" :syntax-word 95 :comment-start "<!-- " :indent mmm-erb-indent-line))"#
        ]],
    )
}

fn manual_regexp_region_can_be_narrowed_cleared_reparsed_and_restored() -> ParityBatchCase {
    ParityBatchCase::value(
        "manual_regexp_region_can_be_narrowed_cleared_reparsed_and_restored",
        r####"
(neomacs-mmm-test-with-buffer
  (insert "plain\nPY{value = 42\nprint(value)}\ntail\n")
  (text-mode)
  (let ((mmm-mode-ext-classes-alist nil)
        (mmm-classes nil)
        (mmm-parse-when-idle nil))
    (mmm-mode-on)
    (mmm-ify-by-regexp 'python-mode "PY{" 0 "}" 0 0)
    (let ((created (neomacs-mmm-test-state)))
      (goto-char (point-min))
      (search-forward "value")
      (mmm-update-current-submode)
      (mmm-narrow-to-submode-region)
      (let ((narrowed (list :text (buffer-string)
                            :restriction (list (point-min) (point-max))
                            :submode mmm-current-submode)))
        (widen)
        (mmm-clear-current-region)
        (let ((cleared (neomacs-mmm-test-overlays)))
          (mmm-parse-buffer)
          (let ((reparsed (neomacs-mmm-test-overlays)))
            (mmm-mode-off)
            (list :created created :narrowed narrowed :cleared cleared
                  :reparsed reparsed :off (neomacs-mmm-test-state))))))))
"####,
        expect![[
            r#"OK (:created (:mode t :major text-mode :primary text-mode :current nil :mode-name "Text" :indent mmm-indent-line :fontifier mmm-fontify-region :syntax mmm-syntax-propertize-function :overlays ((:range (10 33) :text "value = 42\nprint(value)" :mode python-mode :name nil :face mmm-default-submode-face :delimiter nil :special nil)) :bindings (("C-c % c") ("C-c % x") ("C-c % r") ("C-c % b") ("C-c % k") ("C-c % z"))) :narrowed (:text "value = 42\nprint(value)" :restriction (10 33) :submode python-mode) :cleared nil :reparsed ((:range (10 33) :text "value = 42\nprint(value)" :mode python-mode :name nil :face mmm-default-submode-face :delimiter nil :special nil)) :off (:mode nil :major text-mode :primary text-mode :current nil :mode-name "Text" :indent mmm-indent-line :fontifier font-lock-default-fontify-region :syntax mmm-syntax-propertize-function :overlays nil :bindings (("C-c % c") ("C-c % x") ("C-c % r") ("C-c % b") ("C-c % k") ("C-c % z"))))"#
        ]],
    )
}

fn indentation_dispatches_between_html_and_multiline_ruby_blocks() -> ParityBatchCase {
    ParityBatchCase::value(
        "indentation_dispatches_between_html_and_multiline_ruby_blocks",
        r####"
(neomacs-mmm-test-with-buffer
  (insert "<div>\n<% if ready\nputs :ship\nend %>\n<span>done</span>\n</div>\n")
  (html-erb-mode)
  ;; Load Ruby mode before lexically binding its user option: the mode declares
  ;; this variable special, and loading it inside the binding is invalid.
  (require 'ruby-mode)
  (let ((mmm-mode-ext-classes-alist nil)
        (mmm-classes '(erb))
        (mmm-parse-when-idle nil)
        (sgml-basic-offset 2)
        (ruby-indent-level 2))
    (mmm-mode-on)
    (indent-region (point-min) (point-max))
    (list :text (buffer-string)
          :lines
          (save-excursion
            (goto-char (point-min))
            (let (result)
              (while (not (eobp))
                (push (list (line-number-at-pos) (current-indentation)
                            (mmm-submode-at (line-beginning-position))) result)
                (forward-line 1))
              (nreverse result))))))
"####,
        expect![[
            r#"OK (:text "<div>\n  <% if ready\n       puts :ship\n     end %>\n    <span>done</span>\n</div>\n" :lines ((1 0 nil) (2 2 nil) (3 7 ruby-mode) (4 5 ruby-mode) (5 4 nil) (6 0 nil)))"#
        ]],
    )
}

fn mode_extension_configuration_auto_enables_erb_for_matching_files_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_extension_configuration_auto_enables_erb_for_matching_files_only",
        r####"
(let ((mmm-mode-ext-classes-alist nil)
      (mmm-global-mode 'maybe)
      (mmm-parse-when-idle nil))
  (mmm-add-mode-ext-class 'html-erb-mode "\\.html\\.erb\\'" 'erb)
  (list
   :matching
   (neomacs-mmm-test-with-buffer
     (setq buffer-file-name "/workspace/release.html.erb")
     (insert "<%= release %>")
     (html-erb-mode)
     (mmm-mode-on-maybe)
     (list :mode mmm-mode :classes (mmm-get-mode-ext-classes)
           :overlays (neomacs-mmm-test-overlays)))
   :nonmatching
   (neomacs-mmm-test-with-buffer
     (setq buffer-file-name "/workspace/release.html")
     (insert "<%= release %>")
     (html-erb-mode)
     (mmm-mode-on-maybe)
     (list :mode mmm-mode :classes (mmm-get-mode-ext-classes)
           :overlays (neomacs-mmm-test-overlays)))))
"####,
        expect![[
            r#"OK (:matching (:mode t :classes (erb) :overlays ((:range (4 13) :text " release " :mode ruby-mode :name nil :face mmm-default-submode-face :delimiter nil :special t))) :nonmatching (:mode nil :classes nil :overlays nil))"#
        ]],
    )
}

fn invalid_classes_are_collected_after_valid_classes_still_apply() -> ParityBatchCase {
    ParityBatchCase::value(
        "invalid_classes_are_collected_after_valid_classes_still_apply",
        r####"
(neomacs-mmm-test-with-buffer
  (insert "<%= value %>")
  (html-erb-mode)
  (let ((mmm-mode-ext-classes-alist nil)
        (mmm-parse-when-idle nil))
    (mmm-mode-on)
    (let ((outcome
           (condition-case err
               (list :value (mmm-apply-classes '(erb missing-one missing-two)))
             (error (list :signal (car err) :data (cdr err)
                          :message (error-message-string err))))))
      (list :outcome outcome :overlays (neomacs-mmm-test-overlays)
            :mode mmm-mode))))
"####,
        expect![[
            r#"OK (:outcome (:signal mmm-invalid-submode-class :data (missing-two missing-one) :message "Invalid or undefined submode class: missing-two, missing-one") :overlays ((:range (4 11) :text " value " :mode ruby-mode :name nil :face mmm-default-submode-face :delimiter nil :special t)) :mode t)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        erb_class_parses_multiple_ruby_regions_with_delimiters_faces_and_font_lock(),
        point_transitions_switch_submode_keymap_syntax_and_mode_line_then_restore_primary(),
        manual_regexp_region_can_be_narrowed_cleared_reparsed_and_restored(),
        indentation_dispatches_between_html_and_multiline_ruby_blocks(),
        mode_extension_configuration_auto_enables_erb_for_matching_files_only(),
        invalid_classes_are_collected_after_valid_classes_still_apply(),
    ]
}
