use expect_test::expect;

use super::ParityBatchCase;

fn real_pinned_grammars_activate_both_parsers_and_the_complete_mode_integration() -> ParityBatchCase
{
    ParityBatchCase::value(
        "real_pinned_grammars_activate_both_parsers_and_the_complete_mode_integration",
        r##"(with-temp-buffer
  (insert
   "= Practical Document\n"
   ":author: Ada\n\n"
   "== First Section\n\n"
   "A paragraph with *bold* and `code`.\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (list
   major-mode
   mode-name
   (derived-mode-p 'text-mode)
   (treesit-available-p)
   (treesit-language-available-p 'asciidoc)
   (treesit-language-available-p 'asciidoc-inline)
   (mapcar
    #'treesit-parser-language
    (treesit-parser-list))
   (treesit-parser-language treesit-primary-parser)
   treesit-font-lock-feature-list
   treesit-simple-imenu-settings
   treesit-defun-type-regexp
   treesit-outline-predicate
   outline-minor-mode
   outline-minor-mode-cycle
   (memq #'asciidoc--xref-backend
         xref-backend-functions)
   (memq #'asciidoc--capf
         completion-at-point-functions)
   (memq #'asciidoc-flymake
         flymake-diagnostic-functions)
   comment-start
   comment-start-skip))"##,
        expect![[
            r#"OK (asciidoc-mode "AsciiDoc" text-mode t t t (asciidoc asciidoc-inline) asciidoc ((comment title) (block delimiter table list attribute macro metadata) (inline-markup inline-link inline-macro inline-reference) (replacement)) (("Section" "\\`title[1-5]\\'" nil asciidoc--imenu-name)) "\\`\\(?:document_title\\|title[1-5]\\)\\'" "\\`\\(?:document_title\\|title[1-5]\\)\\'" t t (asciidoc--xref-backend t) (asciidoc--capf t ispell-completion-at-point) (asciidoc-flymake t) "// " "^//+\\s-*")"#
        ]],
    )
}

fn grammar_install_command_installs_only_missing_languages_with_exact_recipes_and_messages()
-> ParityBatchCase {
    ParityBatchCase::value(
        "grammar_install_command_installs_only_missing_languages_with_exact_recipes_and_messages",
        r##"(let ((available '(asciidoc))
       installs
       messages
       source-alists)
  (cl-letf
      (((symbol-function
         'treesit-language-available-p)
        (lambda (language)
          (memq language available)))
       ((symbol-function
         'treesit-install-language-grammar)
        (lambda (language)
          (push language installs)
          (push
           (copy-tree
            treesit-language-source-alist)
           source-alists)
          (push language available)
          'installed))
       ((symbol-function 'message)
        (lambda (format-string &rest arguments)
          (push
           (apply #'format
                  format-string arguments)
           messages))))
    (list
     (asciidoc-install-grammars)
     (nreverse installs)
     (nreverse messages)
     (nreverse source-alists)
     available)))"##,
        expect![[
            r#"OK (nil (asciidoc-inline) ("Installing tree-sitter grammar for asciidoc-inline..." "Installing tree-sitter grammar for asciidoc-inline...done") (((asciidoc "https://github.com/cathaysia/tree-sitter-asciidoc" nil "tree-sitter-asciidoc/src") (asciidoc-inline "https://github.com/cathaysia/tree-sitter-asciidoc" nil "tree-sitter-asciidoc_inline/src"))) (asciidoc-inline asciidoc))"#
        ]],
    )
}

fn grammar_install_command_propagates_install_failure_without_attempting_later_messages()
-> ParityBatchCase {
    ParityBatchCase::value(
        "grammar_install_command_propagates_install_failure_without_attempting_later_messages",
        r##"(let (calls)
  (cl-letf
      (((symbol-function
         'treesit-language-available-p)
        (lambda (language)
          (push (list 'available language) calls)
          nil))
       ((symbol-function
         'treesit-install-language-grammar)
        (lambda (language)
          (push (list 'install language) calls)
          (error "compiler rejected %s" language)))
       ((symbol-function 'message)
        (lambda (format-string &rest arguments)
          (push
           (list 'message
                 (apply #'format
                        format-string arguments))
           calls))))
    (condition-case error
        (asciidoc-install-grammars)
      (error
       (list
        (car error)
        (cdr error)
        (nreverse calls))))))"##,
        expect![[
            r#"OK (error ("compiler rejected asciidoc") ((available asciidoc) (message "Installing tree-sitter grammar for asciidoc...") (install asciidoc)))"#
        ]],
    )
}

fn grammarless_fallback_remains_a_usable_text_mode_with_comments_filling_and_flymake()
-> ParityBatchCase {
    ParityBatchCase::value(
        "grammarless_fallback_remains_a_usable_text_mode_with_comments_filling_and_flymake",
        r##"(cl-letf
    (((symbol-function 'asciidoc--ensure-grammars)
      (lambda () nil)))
  (with-temp-buffer
    (insert
     "= Fallback Document\n\n"
     "Visit https://example.com/a/b for practical details about the project.\n")
    (asciidoc-mode)
    (setq-local fill-column 38)
    (goto-char (point-min))
    (forward-line 2)
    (fill-paragraph)
    (list
     major-mode
     (derived-mode-p 'text-mode)
     (treesit-parser-list)
     outline-minor-mode
     (memq #'asciidoc--xref-backend
           xref-backend-functions)
     (memq #'asciidoc--capf
           completion-at-point-functions)
     (memq #'asciidoc-flymake
           flymake-diagnostic-functions)
     comment-start
     comment-start-skip
     (buffer-string)
     (string-match-p
      "^[ \t]*//"
      (buffer-string)))))"##,
        expect![[
            r#"OK (asciidoc-mode text-mode nil nil nil nil (asciidoc-flymake t) "// " "^//+\\s-*" "= Fallback Document\n\nVisit https://example.com/a/b for\npractical details about the project.\n" nil)"#
        ]],
    )
}

pub(super) fn activation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        real_pinned_grammars_activate_both_parsers_and_the_complete_mode_integration(),
        grammar_install_command_installs_only_missing_languages_with_exact_recipes_and_messages(),
        grammar_install_command_propagates_install_failure_without_attempting_later_messages(),
        grammarless_fallback_remains_a_usable_text_mode_with_comments_filling_and_flymake(),
    ]
}
