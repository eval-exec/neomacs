use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_auctex_exact_descriptor_dependency_versions_and_payload_bytes_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_exact_descriptor_dependency_versions_and_payload_bytes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-complete-auctex
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor)))
          (list
           (package-desc-name descriptor)
           (package-version-join
            (package-desc-version descriptor))
           (package-desc-summary descriptor)
           (package-desc-reqs descriptor)
           (package-desc-kind descriptor)
           (package-desc-extras descriptor)
           (mapcar
            (lambda (package)
              (let ((dependency
                     (cadr
                      (assq package package-alist))))
                (list
                 package
                 (and
                  dependency
                  (package-version-join
                   (package-desc-version dependency)))
                 (and
                  dependency
                  (file-name-nondirectory
                   (directory-file-name
                    (package-desc-dir dependency)))))))
            '(auto-complete popup yasnippet auctex))
           (mapcar
            (lambda (file)
              (let ((path
                     (expand-file-name file directory)))
                (list
                 file
                 (file-attribute-size
                  (file-attributes path))
                 (with-temp-buffer
                   (insert-file-contents-literally path)
                   (secure-hash
                    'sha256
                    (current-buffer))))))
            '("auto-complete-auctex-pkg.el"
              "auto-complete-auctex.el"))))"##,
        expect![[
            r#"OK (auto-complete-auctex "20140223.1758" "Auto-completion for auctex." ((yasnippet (0 6 1)) (auto-complete (1 4))) nil ((:maintainers ("Christopher Monsanto" . "chris@monsan.to")) (:authors ("Christopher Monsanto" . "chris@monsan.to")) (:revdesc . "855633f668bc") (:commit . "855633f668bcc4b9408396742a7cb84e0c4a2f77") (:url . "https://github.com/emacsattic/auto-complete-auctex")) ((auto-complete "20251231.1622" "auto-complete-20251231.1622") (popup "20251231.1622" "popup-20251231.1622") (yasnippet "20250602.1342" "yasnippet-20250602.1342") (auctex "14.1.2" "auctex-14.1.2")) (("auto-complete-auctex-pkg.el" 456 "3908112169feb50751557f71d269c9ef4d50c60797a3a41375a6b8f4b823ce24") ("auto-complete-auctex.el" 7453 "e943b5613b8fdafecce3b2ff5025962171548022a73121fcb38ad2184824304f")))"#
        ]],
    )
}

fn auto_complete_auctex_complete_public_and_private_symbol_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_complete_public_and_private_symbol_surface_matches",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (when
                 (string-prefix-p
                  "ac-auctex-"
                  (symbol-name symbol))
               (unless
                   (string-prefix-p
                    "ac-auctex-test-"
                    (symbol-name symbol))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (boundp symbol)
                   (and
                    (commandp symbol)
                    t)
                   (file-name-base
                    (or
                     (symbol-file symbol)
                     "")))
                  symbols)))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name
               (car left))
              (symbol-name
               (car right))))))"##,
        expect![[
            r#"OK ((ac-auctex-arg-lookup-table nil t nil "auto-complete-auctex") (ac-auctex-bib-candidates t nil nil "auto-complete-auctex") (ac-auctex-environment-action t nil nil "auto-complete-auctex") (ac-auctex-environment-candidates t nil nil "auto-complete-auctex") (ac-auctex-environment-prefix nil t nil "auto-complete-auctex") (ac-auctex-expand-arg-info t nil nil "auto-complete-auctex") (ac-auctex-expand-args t nil nil "auto-complete-auctex") (ac-auctex-label-candidates t nil nil "auto-complete-auctex") (ac-auctex-macro-action t nil nil "auto-complete-auctex") (ac-auctex-macro-candidates t nil nil "auto-complete-auctex") (ac-auctex-macro-snippet t nil nil "auto-complete-auctex") (ac-auctex-setup t nil nil "auto-complete-auctex") (ac-auctex-snippet-arg t nil nil "auto-complete-auctex") (ac-auctex-symbol-action t nil nil "auto-complete-auctex") (ac-auctex-symbol-candidates t nil nil "auto-complete-auctex") (ac-auctex-symbol-document t nil nil "auto-complete-auctex"))"#
        ]],
    )
}

fn auto_complete_auctex_every_callable_arglist_and_source_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_every_callable_arglist_and_source_match",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (help-function-arglist symbol t)
             (and
              (interactive-form symbol)
              t)
             (file-name-base
              (or
               (symbol-file symbol 'defun)
               ""))))
          '(ac-auctex-expand-arg-info
            ac-auctex-snippet-arg
            ac-auctex-expand-args
            ac-auctex-macro-snippet
            ac-auctex-macro-candidates
            ac-auctex-macro-action
            ac-auctex-symbol-candidates
            ac-auctex-symbol-action
            ac-auctex-symbol-document
            ac-auctex-environment-candidates
            ac-auctex-environment-action
            ac-auctex-label-candidates
            ac-auctex-bib-candidates
            ac-auctex-setup))"##,
        expect![[
            r#"OK ((ac-auctex-expand-arg-info (arg-info) nil "auto-complete-auctex") (ac-auctex-snippet-arg (n arg) nil "auto-complete-auctex") (ac-auctex-expand-args (str env) nil "auto-complete-auctex") (ac-auctex-macro-snippet (arg-info) nil "auto-complete-auctex") (ac-auctex-macro-candidates nil nil "auto-complete-auctex") (ac-auctex-macro-action nil nil "auto-complete-auctex") (ac-auctex-symbol-candidates nil nil "auto-complete-auctex") (ac-auctex-symbol-action nil nil "auto-complete-auctex") (ac-auctex-symbol-document (c) nil "auto-complete-auctex") (ac-auctex-environment-candidates nil nil "auto-complete-auctex") (ac-auctex-environment-action nil nil "auto-complete-auctex") (ac-auctex-label-candidates nil nil "auto-complete-auctex") (ac-auctex-bib-candidates nil nil "auto-complete-auctex") (ac-auctex-setup nil nil "auto-complete-auctex"))"#
        ]],
    )
}

fn auto_complete_auctex_argument_lookup_table_exact_keys_values_and_digest_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_auctex_argument_lookup_table_exact_keys_values_and_digest_match",
        r##"(list
          (length ac-auctex-arg-lookup-table)
          ac-auctex-arg-lookup-table
          (secure-hash
           'sha256
           (prin1-to-string
            ac-auctex-arg-lookup-table)))"##,
        expect![[
            r##"OK (35 ((TeX-arg-define-macro "\\MacroName") (TeX-arg-counter "Counter") (TeX-arg-define-counter "\\CounterName") (TeX-arg-file "Filename") (TeX-arg-bibliography "Filename") (TeX-arg-bibstyle "Style") (TeX-arg-environment "Environment") (TeX-arg-define-environment "EnvironmentName") (TeX-arg-size "(w, h)") (TeX-arg-ref "Name") (TeX-arg-index "Index") (TeX-arg-define-label "Label") (LaTeX-arg-usepackage ["opt1,..."] "Package") (LaTeX-env-label) (LaTeX-amsmath-env-aligned ["htbp!"]) (LaTeX-amsmath-env-alignat ["# Columns"]) (LaTeX-env-array ["bct"] "lcrpmb|") (LaTeX-env-item) (LaTeX-env-document) (LaTeX-env-figure ["htbp!"]) (LaTeX-env-contents "Filename") (LaTeX-env-minipage ["htbp!"] "Width") (LaTeX-env-list "Label" "\\itemsep,\\labelsep,...") (LaTeX-env-picture "(w, h)" "(x, y)") (LaTeX-env-tabular* "Width" ["htbp!"] "lcrpmb|><") (LaTeX-env-bib "WidestLabel") (TeX-arg-conditional [""]) (2 "" "") (3 "" "" "") (4 "" "" "" "") (5 "" "" "" "" "") (6 "" "" "" "" "" "") (7 "" "" "" "" "" "" "") (8 "" "" "" "" "" "" "" "") (9 "" "" "" "" "" "" "" "" "")) "7a0689fec2cffe6ad228df562f2b0bce8629fd5f6339a8b6f87db8a45880b97b")"##
        ]],
    )
}

fn auto_complete_auctex_generated_sources_have_exact_completion_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_generated_sources_have_exact_completion_contracts",
        r##"(mapcar
          (lambda (source)
            (list
             source
             (symbol-value source)
             (fboundp
              (intern
               (substring
                (symbol-name source)
                (length "ac-source-"))))))
          '(ac-source-auctex-macros
            ac-source-auctex-symbols
            ac-source-auctex-environments
            ac-source-auctex-labels
            ac-source-auctex-bibs))"##,
        expect![[
            r#"OK ((ac-source-auctex-macros ((init . TeX-symbol-list) (candidates . ac-auctex-macro-candidates) (action . ac-auctex-macro-action) (requires . 0) (symbol . "m") (prefix . "\\\\\\([a-zA-Z]*\\)\\=")) nil) (ac-source-auctex-symbols ((init . LaTeX-math-mode) (candidates . ac-auctex-symbol-candidates) (document . ac-auctex-symbol-document) (action . ac-auctex-symbol-action) (requires . 0) (symbol . "s") (prefix . "\\\\\\([a-zA-Z]*\\)\\=")) nil) (ac-source-auctex-environments ((init . LaTeX-environment-list) (candidates . ac-auctex-environment-candidates) (action . ac-auctex-environment-action) (requires . 0) (symbol . "e") (prefix . "\\\\\\([a-zA-Z]*\\)\\=")) nil) (ac-source-auctex-labels ((init . LaTeX-label-list) (candidates . ac-auctex-label-candidates) (requires . 0) (symbol . "r") (prefix . "\\\\ref{\\([^}]*\\)\\=")) nil) (ac-source-auctex-bibs ((init . LaTeX-bibitem-list) (candidates . ac-auctex-bib-candidates) (requires . 0) (symbol . "b") (prefix . "\\\\cite\\(?:\\[[^]]*\\]\\)?{\\([^},]*\\)\\=")) nil))"#
        ]],
    )
}

fn auto_complete_auctex_load_registers_feature_mode_and_auctex_hook_exactly_once() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_auctex_load_registers_feature_mode_and_auctex_hook_exactly_once",
        r##"(list
          (featurep 'auto-complete-auctex)
          (memq 'latex-mode ac-modes)
          (length
           (seq-filter
            (lambda (function)
              (eq function 'ac-auctex-setup))
            LaTeX-mode-hook))
          (file-name-base
           (or
            (symbol-file 'auto-complete-auctex)
            ""))
          (mapcar
           #'file-name-base
           (delq
            nil
            (mapcar
             (lambda (entry)
               (and
                (member
                 'auto-complete-auctex
                 (cdr entry))
                (car entry)))
             load-history))))"##,
        expect![[
            r#"OK (t (latex-mode emacs-lisp-mode lisp-mode lisp-interaction-mode slime-repl-mode nim-mode c-mode cc-mode c++-mode objc-mode swift-mode go-mode java-mode malabar-mode clojure-mode clojurescript-mode scala-mode scheme-mode ocaml-mode tuareg-mode coq-mode haskell-mode agda-mode agda2-mode perl-mode cperl-mode python-mode ruby-mode lua-mode tcl-mode ecmascript-mode javascript-mode js-mode js-jsx-mode js2-mode js2-jsx-mode coffee-mode php-mode css-mode scss-mode less-css-mode elixir-mode makefile-mode sh-mode fortran-mode f90-mode ada-mode xml-mode sgml-mode web-mode ts-mode sclang-mode verilog-mode qml-mode apples-mode) 1 "auto-complete-auctex" nil)"#
        ]],
    )
}

fn auto_complete_auctex_current_yasnippet_dependency_retains_the_legacy_entry_point_alias()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_auctex_current_yasnippet_dependency_retains_the_legacy_entry_point_alias",
        r##"(list
          (fboundp 'yas/expand-snippet)
          (fboundp 'yas-expand-snippet)
          (symbol-function
           'yas/expand-snippet)
          (file-name-base
           (or
            (symbol-file 'yas-expand-snippet)
            ""))
          (let ((candidate
                 "section")
                (TeX-symbol-list
                 '(("section" "Title"))))
            (ac-auctex-test-error-data
             (lambda ()
               (ac-auctex-macro-action)))))"##,
        expect![[
            r#"OK (t t yas-expand-snippet "yasnippet" (:signal error ("[yas] ‘yas-expand-snippet’ needs properly setup ‘yas-minor-mode’")))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn registry_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_auctex_exact_descriptor_dependency_versions_and_payload_bytes_match(),
        auto_complete_auctex_complete_public_and_private_symbol_surface_matches(),
        auto_complete_auctex_every_callable_arglist_and_source_match(),
        auto_complete_auctex_argument_lookup_table_exact_keys_values_and_digest_match(),
        auto_complete_auctex_generated_sources_have_exact_completion_contracts(),
        auto_complete_auctex_load_registers_feature_mode_and_auctex_hook_exactly_once(),
        auto_complete_auctex_current_yasnippet_dependency_retains_the_legacy_entry_point_alias(),
    ]
}
