use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_config_additional_sources_and_generated_commands_match_exact_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_config_additional_sources_and_generated_commands_match_exact_contracts",
        r##"(mapcar
                          (lambda (pair)
                            (let ((source (car pair))
                                  (command (cdr pair)))
                              (list
                               source
                               (symbol-value source)
                               command
                               (interactive-form command))))
                          '((ac-source-imenu
                             . ac-complete-imenu)
                            (ac-source-gtags
                             . ac-complete-gtags)
                            (ac-source-yasnippet
                             . ac-complete-yasnippet)
                            (ac-source-semantic
                             . ac-complete-semantic)
                            (ac-source-semantic-raw
                             . ac-complete-semantic-raw)
                            (ac-source-eclim
                             . ac-complete-eclim)
                            (ac-source-css-property
                             . ac-complete-css-property)
                            (ac-source-slime
                             . ac-complete-slime)
                            (ac-source-ghc-mod
                             . ac-complete-ghc-mod)))"##,
        expect![[
            r#"OK ((ac-source-imenu ((depends imenu) (candidates . ac-imenu-candidates) (symbol . "s")) ac-complete-imenu (interactive nil)) (ac-source-gtags ((candidates . ac-gtags-candidate) (candidate-face . ac-gtags-candidate-face) (selection-face . ac-gtags-selection-face) (requires . 3) (symbol . "s")) ac-complete-gtags (interactive nil)) (ac-source-yasnippet ((depends yasnippet) (candidates . ac-yasnippet-candidates) (action . yas/expand) (candidate-face . ac-yasnippet-candidate-face) (selection-face . ac-yasnippet-selection-face) (symbol . "a")) ac-complete-yasnippet (interactive nil)) (ac-source-semantic ((available or (require 'semantic-ia nil t) (require 'semantic/ia nil t)) (candidates ac-semantic-candidates ac-prefix) (document . ac-semantic-doc) (action . ac-semantic-action) (prefix . cc-member) (requires . 0) (symbol . "m")) ac-complete-semantic (interactive nil)) (ac-source-semantic-raw ((available or (require 'semantic-ia nil t) (require 'semantic/ia nil t)) (candidates ac-semantic-candidates ac-prefix) (document . ac-semantic-doc) (action . ac-semantic-action) (symbol . "s")) ac-complete-semantic-raw (interactive nil)) (ac-source-eclim ((candidates . ac-eclim-candidates) (prefix . c-dot) (requires . 0) (symbol . "f")) ac-complete-eclim (interactive nil)) (ac-source-css-property ((candidates . ac-css-property-candidates) (prefix . ac-css-prefix) (requires . 0)) ac-complete-css-property (interactive nil)) (ac-source-slime ((depends slime) (candidates car (slime-simple-completions ac-prefix)) (symbol . "s") (cache)) ac-complete-slime (interactive nil)) (ac-source-ghc-mod ((depends ghc) (candidates ghc-select-completion-symbol) (symbol . "s") (cache)) ac-complete-ghc-mod (interactive nil)))"#
        ]],
    )
}

fn auto_complete_imenu_candidates_flatten_nested_index_strip_suffixes_and_honor_prefix_and_limit()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_imenu_candidates_flatten_nested_index_strip_suffixes_and_honor_prefix_and_limit",
        r##"(with-temp-buffer
                          (require 'imenu)
                          (emacs-lisp-mode)
                          (setq-local
                           imenu-create-index-function
                           (lambda ()
                             '(("*Rescan*" . -99)
                               ("Functions"
                                ("alpha()" . 10)
                                ("alpine<>" . 20)
                                ("beta=" . 30))
                               ("Variables"
                                ("alpha-value" . 40)
                                ("gamma" . 50)))))
                          (setq-local imenu--index-alist nil)
                          (mapcar
                           (lambda (case)
                             (setq
                              ac-imenu-index nil
                              ac-prefix (car case)
                              ac-limit (cdr case))
                             (list
                              case
                              (ac-imenu-candidates)
                              ac-imenu-index))
                           '(("a")
                             ("a" . 2)
                             ("b")
                             ("z"))))"##,
        expect![[
            r#"OK ((("a") ("alpine" "alpha" "alpha-value") (#1=("*Rescan*" . -99) . #2=(("*Rescan*" . -99) ("Functions" ("alpha()" . 10) ("alpine<>" . 20) ("beta=" . 30)) ("Variables" ("alpha-value" . 40) ("gamma" . 50))))) (("a" . 2) ("alpine" "alpha") (#1# . #2#)) (("b") ("beta") (#1# . #2#)) (("z") nil (#1# . #2#)))"#
        ]],
    )
}

fn auto_complete_css_source_returns_real_properties_expanded_values_and_pseudo_classes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_css_source_returns_real_properties_expanded_values_and_pseudo_classes",
        r##"(mapcar
                          (lambda (property)
                            (let ((ac-css-property property))
                              (list
                               property
                               (ac-css-property-candidates))))
                          '(t
                            "background"
                            "border"
                            "font"
                            "font-family"
                            "list-style"
                            "unknown-property"))"##,
        expect![[
            r#"OK ((t ("azimuth" "background" "background-attachment" "background-color" "background-image" "background-position" "background-repeat" "border" "border-bottom" "border-bottom-color" "border-bottom-style" "border-bottom-width" "border-collapse" "border-color" "border-left" "border-left-color" "border-left-style" "border-left-width" "border-right" "border-right-color" "border-right-style" "border-right-width" "border-spacing" "border-style" "border-top" "border-top-color" "border-top-style" "border-top-width" "border-width" "bottom" "caption-side" "clear" "clip" "color" "content" "counter-increment" "counter-reset" "cue" "cue-after" "cue-before" "cursor" "direction" "display" "elevation" "empty-cells" "float" "font" "font-family" "font-size" "font-style" "font-variant" "font-weight" "height" "left" "letter-spacing" "line-height" "list-style" "list-style-image" "list-style-position" "list-style-type" "margin" "margin-bottom" "margin-left" "margin-right" "margin-top" "max-height" "max-width" "min-height" "min-width" "orphans" "outline" "outline-color" "outline-style" "outline-width" "overflow" "padding" "padding-bottom" "padding-left" "padding-right" "padding-top" "page-break-after" "page-break-before" "page-break-inside" "pause" "pause-after" "pause-before" "pitch" "pitch-range" "play-during" "position" "quotes" "richness" "right" "speak" "speak-header" "speak-numeral" "speak-punctuation" "speech-rate" "stress" "table-layout" "text-align" "text-decoration" "text-indent" "text-transform" "top" "unicode-bidi" "vertical-align" "visibility" "voice-family" "volume" "white-space" "widows" "width" "word-spacing" "z-index")) ("background" ("transparent" "none" "repeat" "repeat-x" "repeat-y" "no-repeat" "scroll" "fixed" "left" "center" "right" "top" "center" "bottom" "left" "center" "right" "top" "center" "bottom" "aqua" "black" "blue" "fuchsia" "gray" "green" "lime" "maroon" "navy" "olive" "orange" "purple" "red" "silver" "teal" "white" "yellow" "rgb" "url")) ("border" ("none" "hidden" "dotted" "dashed" "solid" "double" "groove" "ridge" "inset" "outset" "transparent" "aqua" "black" "blue" "fuchsia" "gray" "green" "lime" "maroon" "navy" "olive" "orange" "purple" "red" "silver" "teal" "white" "yellow" "rgb")) ("font" ("/" "caption" "icon" "menu" "message-box" "small-caption" "status-bar" "normal" "italic" "oblique" "normal" "small-caps" "normal" "bold" "bolder" "lighter" "100" "200" "300" "400" "500" "600" "700" "800" "900" "normal" "xx-small" "x-small" "small" "medium" "large" "x-large" "xx-large" "larger" "smaller" "Courier" "Helvetica" "Times" "serif" "sans-serif" "cursive" "fantasy" "monospace")) ("font-family" ("Courier" "Helvetica" "Times" "serif" "sans-serif" "cursive" "fantasy" "monospace")) ("list-style" ("disc" "circle" "square" "decimal" "decimal-leading-zero" "lower-roman" "upper-roman" "lower-greek" "lower-latin" "upper-latin" "armenian" "georgian" "lower-alpha" "upper-alpha" "none" "inside" "outside" "none" "url")) ("unknown-property" ("active" "after" "before" "first" "first-child" "first-letter" "first-line" "focus" "hover" "lang" "left" "link" "right" "visited")))"#
        ]],
    )
}

fn auto_complete_css_prefix_tracks_property_or_property_name_across_real_declarations()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_css_prefix_tracks_property_or_property_name_across_real_declarations",
        r##"(mapcar
                          (lambda (text)
                            (with-temp-buffer
                              (css-mode)
                              (insert text)
                              (setq ac-css-property nil)
                              (let ((prefix
                                     (ac-css-prefix)))
                                (list
                                 text
                                 prefix
                                 (and
                                  prefix
                                  (buffer-substring-no-properties
                                   prefix
                                   (point)))
                                 ac-css-property))))
                          '("body { back"
                            "body { background: re"
                            "body { color: rgb; mar"
                            ".item:hover { font-family: san"
                            "/* color: red */"
                            ""))"##,
        expect![[
            r#"OK (("body { back" 8 "back" t) ("body { background: re" 20 "re" "background") ("body { color: rgb; mar" 20 "mar" t) (".item:hover { font-family: san" 28 "san" "font-family") ("/* color: red */" 17 "" "color") ("" 1 "" t))"#
        ]],
    )
}

fn auto_complete_config_default_installs_expected_hooks_sources_and_global_mode_behavior()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_config_default_installs_expected_hooks_sources_and_global_mode_behavior",
        r##"(progn
                          (global-auto-complete-mode -1)
                          (remove-hook
                           'emacs-lisp-mode-hook
                           'ac-emacs-lisp-mode-setup)
                          (remove-hook
                           'c-mode-common-hook
                           'ac-cc-mode-setup)
                          (remove-hook
                           'ruby-mode-hook
                           'ac-ruby-mode-setup)
                          (remove-hook
                           'css-mode-hook
                           'ac-css-mode-setup)
                          (remove-hook
                           'auto-complete-mode-hook
                           'ac-common-setup)
                          (let ((before
                                 (list
                                  global-auto-complete-mode
                                  (default-value
                                   'ac-sources))))
                            (ac-config-default)
                            (let ((eligible
                                   (generate-new-buffer
                                    " *ac-config-elisp*"))
                                  (ineligible
                                   (generate-new-buffer
                                    " *ac-config-text*")))
                              (unwind-protect
                                  (progn
                                    (with-current-buffer eligible
                                      (emacs-lisp-mode))
                                    (with-current-buffer ineligible
                                      (text-mode))
                                    (list
                                     before
                                     global-auto-complete-mode
                                     (default-value
                                      'ac-sources)
                                     (memq
                                      'ac-emacs-lisp-mode-setup
                                      emacs-lisp-mode-hook)
                                     (memq
                                      'ac-cc-mode-setup
                                      c-mode-common-hook)
                                     (memq
                                      'ac-css-mode-setup
                                      css-mode-hook)
                                     (memq
                                      'ac-common-setup
                                      auto-complete-mode-hook)
                                     (with-current-buffer eligible
                                       (list
                                        auto-complete-mode
                                        ac-sources))
                                     (with-current-buffer ineligible
                                       (list
                                        auto-complete-mode
                                        ac-sources))))
                                (kill-buffer eligible)
                                (kill-buffer ineligible)
                                (global-auto-complete-mode
                                 -1)))))"##,
        expect![
            "OK ((nil (ac-source-words-in-same-mode-buffers)) t #1=(ac-source-abbrev ac-source-dictionary ac-source-words-in-same-mode-buffers) (ac-emacs-lisp-mode-setup) (ac-cc-mode-setup) (ac-css-mode-setup) (ac-common-setup auto-complete-mode--set-explicitly) (t (ac-source-words-in-same-mode-buffers ac-source-dictionary ac-source-abbrev ac-source-features ac-source-functions ac-source-yasnippet ac-source-variables ac-source-symbols)) (nil #1#))"
        ],
    )
}

fn auto_complete_mode_specific_setup_functions_merge_sources_without_duplicates() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_mode_specific_setup_functions_merge_sources_without_duplicates",
        r##"(mapcar
                          (lambda (setup)
                            (with-temp-buffer
                              (setq
                               ac-sources
                               '(ac-source-symbols
                                 ac-source-yasnippet))
                              (funcall setup)
                              (list setup ac-sources)))
                          '(ac-emacs-lisp-mode-setup
                            ac-cc-mode-setup
                            ac-ruby-mode-setup
                            ac-css-mode-setup
                            ac-common-setup))"##,
        expect![
            "OK ((ac-emacs-lisp-mode-setup (ac-source-features ac-source-functions ac-source-yasnippet ac-source-variables ac-source-symbols)) (ac-cc-mode-setup (ac-source-symbols ac-source-yasnippet ac-source-gtags)) (ac-ruby-mode-setup #1=(ac-source-symbols ac-source-yasnippet)) (ac-css-mode-setup (ac-source-css-property . #1#)) (ac-common-setup #1#))"
        ],
    )
}

fn auto_complete_yasnippet_source_supports_modern_active_keys_and_legacy_parent_tables()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_yasnippet_source_supports_modern_active_keys_and_legacy_parent_tables",
        r##"(let ((modern-calls 0))
                           (fset
                            'yas-active-keys
                            (lambda ()
                              (setq modern-calls
                                    (1+ modern-calls))
                              '("for"
                                "foreach"
                                "if"
                                "lambda")))
                           (let ((ac-prefix "fo"))
                             (let ((modern
                                    (ac-yasnippet-candidates)))
                               (fmakunbound
                                'yas-active-keys)
                               (fset
                                'yas/snippet-table-hash
                                (lambda (table)
                                  (car table)))
                               (fset
                                'yas/snippet-table-parent
                                (lambda (table)
                                  (cdr table)))
                               (let* ((parent-hash
                                      (make-hash-table
                                       :test 'equal))
                                     (child-hash
                                      (make-hash-table
                                       :test 'equal))
                                     (parent
                                      (cons parent-hash nil))
                                     (child
                                      (cons child-hash parent)))
                                 (puthash
                                  "format"
                                  "parent"
                                  parent-hash)
                                 (puthash
                                  "forward"
                                  "child"
                                  child-hash)
                                 (puthash
                                  "if"
                                  "ignored"
                                  child-hash)
                                 (list
                                  modern-calls
                                  modern
                                  (ac-yasnippet-candidate-1
                                   child))))))"##,
        expect![[r#"OK (1 ("for" "foreach") ("forward" "format"))"#]],
    )
}

fn auto_complete_gtags_and_eclim_sources_transform_external_results_at_their_process_seams()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_gtags_and_eclim_sources_transform_external_results_at_their_process_seams",
        r##"(progn
                          (fset
                           'shell-command-to-string
                           (lambda (command)
                             (setq
                              auto-complete-test-shell-command
                              command)
                             "alpha\nalpine\n\n"))
                          (fset
                           'eclim/java-complete
                           (lambda ()
                             '(("kind" "java.util.List")
                               ("kind"
                                "java.util.LinkedList"))))
                          (let ((ac-prefix "java.util.L"))
                            (list
                             auto-complete-test-shell-command
                             (ac-gtags-candidate)
                             auto-complete-test-shell-command
                             (ac-eclim-candidates))))"##,
        expect![[
            r#"OK (nil ("alpha" "alpine" "" "") "global -ciq java.util.L" ("java.util.List" "java.util.LinkedList"))"#
        ]],
    )
}

fn auto_complete_semantic_source_preserves_candidate_values_and_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_semantic_source_preserves_candidate_values_and_documentation",
        r##"(progn
                          (fset
                           'semantic-analyze-current-context
                           (lambda ()
                             'fixture-context))
                          (fset
                           'semantic-analyze-possible-completions
                           (lambda (_context)
                             '(fixture-empty
                               fixture-method)))
                          (fset
                           'semantic-tag-name
                           (lambda (tag)
                             (if (eq tag
                                     'fixture-empty)
                                 ""
                               "render")))
                          (fset
                           'semantic-tag-clone
                           (lambda (tag)
                             (list :clone tag)))
                          (fset
                           'semantic-format-tag-summarize-with-file
                           (lambda (tag _parent _color)
                             (format
                              "prototype:%S"
                              tag)))
                          (fset
                           'semantic-documentation-for-tag
                           (lambda (tag)
                             (format
                              "documentation:%S"
                              tag)))
                          (let ((candidates
                                 (ac-semantic-candidates
                                  "ren")))
                            (list
                             (mapcar
                              (lambda (candidate)
                                (list
                                 (car candidate)
                                 (cdr candidate)))
                              candidates)
                             (ac-semantic-doc
                              (cdar candidates)))))"##,
        expect![[
            r#"OK ((("" (:clone fixture-empty)) ("render" (:clone fixture-method))) "prototype:(:clone fixture-empty)\n\ndocumentation:(:clone fixture-empty)")"#
        ]],
    )
}

fn auto_complete_ropemacs_setup_initialization_and_candidate_cache_follow_real_python_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_ropemacs_setup_initialization_and_candidate_cache_follow_real_python_workflow",
        r##"(progn
                          (setq
                           ac-ropemacs-loaded nil
                           ac-ropemacs-completions-cache
                           nil)
                          (fset
                           'pymacs-load
                           (lambda (&rest arguments)
                             (setq
                              auto-complete-test-pymacs-load
                              arguments)))
                          (fset
                           'rope-completions
                           (lambda ()
                             '("ath"
                               "ure"
                               "arse")))
                          (let ((ac-prefix "p"))
                            (ac-ropemacs-setup)
                            (let ((init
                                   (assoc-default
                                    'init
                                    ac-source-ropemacs)))
                              (funcall init))
                            (list
                             ac-ropemacs-loaded
                             auto-complete-test-pymacs-load
                             ac-omni-completion-sources
                             ac-ropemacs-completions-cache
                             (ac-ropemacs-initialize)
                             (memq
                              'ac-ropemacs-setup
                              python-mode-hook)
                             (mapcar
                              (lambda (symbol)
                                (list
                                 symbol
                                 (symbol-function
                                  symbol)))
                              '(pymacs-apply
                                pymacs-call
                                pymacs-eval
                                pymacs-exec
                                pymacs-load)))))"##,
        expect![[
            r#"OK (t ("ropemacs" "rope-") (("\\." ac-source-ropemacs)) ("path" "pure" "parse") t (ac-ropemacs-setup) ((pymacs-apply (autoload "pymacs" nil nil nil)) (pymacs-call (autoload "pymacs" nil nil nil)) (pymacs-eval (autoload "pymacs" nil t nil)) (pymacs-exec (autoload "pymacs" nil t nil)) (pymacs-load #[(&rest arguments) ((setq auto-complete-test-pymacs-load arguments)) (t)])))"#
        ]],
    )
}

pub(super) fn config_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_config_additional_sources_and_generated_commands_match_exact_contracts(),
        auto_complete_imenu_candidates_flatten_nested_index_strip_suffixes_and_honor_prefix_and_limit(),
        auto_complete_css_source_returns_real_properties_expanded_values_and_pseudo_classes(),
        auto_complete_css_prefix_tracks_property_or_property_name_across_real_declarations(),
        auto_complete_config_default_installs_expected_hooks_sources_and_global_mode_behavior(),
        auto_complete_mode_specific_setup_functions_merge_sources_without_duplicates(),
        auto_complete_yasnippet_source_supports_modern_active_keys_and_legacy_parent_tables(),
        auto_complete_gtags_and_eclim_sources_transform_external_results_at_their_process_seams(),
        auto_complete_semantic_source_preserves_candidate_values_and_documentation(),
        auto_complete_ropemacs_setup_initialization_and_candidate_cache_follow_real_python_workflow(),
    ]
}
