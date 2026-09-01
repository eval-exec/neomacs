use expect_test::expect;

use super::ParityBatchCase;

fn package_loads_exact_release_and_complete_public_data_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_loads_exact_release_and_complete_public_data_surface",
        r##"
(list
 (featurep 'aiken-mode)
 (package-version-join
  (package-desc-version
   (cadr (assq 'aiken-mode package-alist))))
 aiken-keywords
 aiken-operators
 (length aiken-font-lock-keywords)
 (commandp 'aiken-mode)
 (derived-mode-p 'prog-mode))
"##,
        expect![[
            r#"OK (t "20230920.1210" ("if" "else" "when" "is" "fn" "use" "let" "pub" "type" "opaque" "const" "todo" "error" "expect" "test" "trace" "fail" "validator" "and" "or") ("=" "->" ".." "|>" ">=" "<=" ">" "<" "!=" "==" "&&" "||" "!" "+" "-" "/" "*" "%" "?") 7 t prog-mode)"#
        ]],
    )
}

fn auto_mode_selects_aiken_only_for_final_ak_extension() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_mode_selects_aiken_only_for_final_ak_extension",
        r##"
(let ((cases
       '(("validator.ak" . aiken-mode)
         ("src/payment.ak" . aiken-mode)
         ("validator.aka" . fundamental-mode)
         ("validator.ak.bak" . fundamental-mode)
         ("AK" . fundamental-mode))))
  (mapcar
   (lambda (case)
     (with-temp-buffer
       (setq buffer-file-name (concat "/workspace/" (car case)))
       (set-auto-mode)
       (list (car case) major-mode (eq major-mode (cdr case)))))
   cases))
"##,
        expect![[
            r#"OK (("validator.ak" aiken-mode t) ("src/payment.ak" aiken-mode t) ("validator.aka" fundamental-mode t) ("validator.ak.bak" aiken-mode nil) ("AK" fundamental-mode t))"#
        ]],
    )
}

fn entering_mode_configures_prog_editing_comments_words_and_font_lock() -> ParityBatchCase {
    ParityBatchCase::value(
        "entering_mode_configures_prog_editing_comments_words_and_font_lock",
        r##"
(with-temp-buffer
  (aiken-mode)
  (list
   major-mode mode-name
   (derived-mode-p 'prog-mode)
   indent-tabs-mode
   comment-start comment-end comment-start-skip
   comment-use-syntax comment-auto-fill-only-comments
   font-lock-defaults
   (char-syntax ?_)
   (char-syntax ?/)
   (char-syntax ?\n)
   (local-variable-p 'font-lock-defaults)
   (local-variable-p 'comment-start)))
"##,
        expect![[
            r#"OK (aiken-mode "Aiken" prog-mode nil "// " "" "//+ *" t t (aiken-font-lock-keywords) 119 46 62 t t)"#
        ]],
    )
}

fn mode_settings_are_buffer_local_and_do_not_leak_to_neighboring_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_settings_are_buffer_local_and_do_not_leak_to_neighboring_buffers",
        r##"
(let ((aiken (generate-new-buffer "contract.ak"))
      (plain (generate-new-buffer "notes.txt")))
  (unwind-protect
      (progn
        (with-current-buffer aiken
          (setq indent-tabs-mode t
                comment-start "# ")
          (aiken-mode))
        (with-current-buffer plain
          (text-mode)
          (setq-local indent-tabs-mode t))
        (list
         (with-current-buffer aiken
           (list major-mode indent-tabs-mode comment-start
                 (char-syntax ?_)))
         (with-current-buffer plain
           (list major-mode indent-tabs-mode comment-start
                 (char-syntax ?_)))))
    (kill-buffer aiken)
    (kill-buffer plain)))
"##,
        expect![[r##"OK ((aiken-mode nil "// " 119) (text-mode t "# " 95))"##]],
    )
}

fn repeated_mode_activation_resets_local_contract_without_destroying_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "repeated_mode_activation_resets_local_contract_without_destroying_text",
        r##"
(with-temp-buffer
  (insert "fn add(x: Int, y: Int) { x + y }\n")
  (aiken-mode)
  (setq-local comment-start "#! "
              indent-tabs-mode t)
  (aiken-mode)
  (list
   (buffer-string)
   major-mode
   comment-start
   indent-tabs-mode
   (eq (car font-lock-defaults)
       'aiken-font-lock-keywords)
   (= (point) (point-min))))
"##,
        expect![[r#"OK ("fn add(x: Int, y: Int) { x + y }\n" aiken-mode "// " nil t nil)"#]],
    )
}

fn unsupported_formatter_indenter_lsp_and_navigation_apis_remain_absent() -> ParityBatchCase {
    ParityBatchCase::value(
        "unsupported_formatter_indenter_lsp_and_navigation_apis_remain_absent",
        r##"
(let ((compile-command "workspace-defined build"))
  (with-temp-buffer
    (insert "{ first } { second }")
    (aiken-mode)
    (let ((end-function end-of-defun-function))
      (goto-char (point-min))
      (funcall end-function)
      (list
       (mapcar
        #'fboundp
        '(aiken-fmt aiken-format-buffer aiken-indent-line
          aiken-lsp aiken-compile aiken-find-definition))
       indent-line-function
       imenu-generic-expression
       beginning-of-defun-function
       (functionp end-function)
       (point)
       (buffer-substring-no-properties (point-min) (point))
       compile-command
       (local-variable-p 'compile-command)))))
"##,
        expect![[
            r#"OK ((nil nil nil nil nil nil) indent-relative nil nil t 10 "{ first }" "workspace-defined build" nil)"#
        ]],
    )
}

pub(super) fn mode_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        package_loads_exact_release_and_complete_public_data_surface(),
        auto_mode_selects_aiken_only_for_final_ak_extension(),
        entering_mode_configures_prog_editing_comments_words_and_font_lock(),
        mode_settings_are_buffer_local_and_do_not_leak_to_neighboring_buffers(),
        repeated_mode_activation_resets_local_contract_without_destroying_text(),
        unsupported_formatter_indenter_lsp_and_navigation_apis_remain_absent(),
    ]
}
