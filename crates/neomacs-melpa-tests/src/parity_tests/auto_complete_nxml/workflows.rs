use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_nxml_real_nxml_mode_setup_installs_completion_environment() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_real_nxml_mode_setup_installs_completion_environment",
        r##"(with-temp-buffer
         (insert "<?xml version=\"1.0\"?><root><child/></root>")
         (nxml-mode)
         (list
          major-mode
          (bound-and-true-p auto-complete-mode)
          ac-sources
          (memq 'nxml-mode ac-modes)
          (memq 'auto-complete-nxml-ac-start-with-insert
                ac-trigger-commands)
          (key-binding (kbd "SPC"))))"##,
        expect![
            "OK (nxml-mode t (ac-source-nxml-tag ac-source-nxml-attr ac-source-nxml-attr-value ac-source-nxml-css ac-source-nxml-css-property ac-source-nxml-tag-value-by-nxml ac-source-nxml-tag-value-by-myself) (nxml-mode emacs-lisp-mode lisp-mode lisp-interaction-mode slime-repl-mode nim-mode c-mode cc-mode c++-mode objc-mode swift-mode go-mode java-mode malabar-mode clojure-mode clojurescript-mode scala-mode scheme-mode ocaml-mode tuareg-mode coq-mode haskell-mode agda-mode agda2-mode perl-mode cperl-mode python-mode ruby-mode lua-mode tcl-mode ecmascript-mode javascript-mode js-mode js-jsx-mode js2-mode js2-jsx-mode coffee-mode php-mode css-mode scss-mode less-css-mode elixir-mode makefile-mode sh-mode fortran-mode f90-mode ada-mode xml-mode sgml-mode web-mode ts-mode sclang-mode verilog-mode qml-mode apples-mode) (auto-complete-nxml-ac-start-with-insert self-insert-command) auto-complete-nxml-ac-start-with-insert)"
        ],
    )
}

fn auto_complete_nxml_practical_content_completion_reuses_words_from_open_document()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_practical_content_completion_reuses_words_from_open_document",
        r##"(let ((auto-complete-nxml-tag-value-words-hash
                                (make-hash-table :test 'equal))
             (auto-complete-nxml-automatic-p t)
             (this-command 'self-insert-command)
             (ac-prefix "Ne"))
         (with-temp-buffer
           (insert
            "<catalog>"
            "<title>Neomacs Editor</title>"
            "<summary>Native Emacs runtime</summary>"
            "<title>Ne")
           (goto-char (point-max))
           (let ((context
                  (auto-complete-nxml-get-current-context-symbol))
                 (candidates
                  (auto-complete-nxml-get-tag-value-candidates-by-myself)))
             (list
              context
              auto-complete-nxml-buffer-current-tag
              candidates
              (member "Neomacs" candidates)
              (member "Native" candidates)))))"##,
        expect![[
            r#"OK (content "title" ("runtime" "Emacs" . #2=("Native" "Editor" . #1=("Neomacs" ""))) #1# #2#)"#
        ]],
    )
}

fn auto_complete_nxml_practical_attribute_completion_reuses_matching_attribute_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_practical_attribute_completion_reuses_matching_attribute_values",
        r##"(let ((auto-complete-nxml-attr-words-hash-hash
                                (make-hash-table :test 'equal))
             (auto-complete-nxml-automatic-p t)
             (this-command 'self-insert-command)
             (ac-prefix "pri"))
         (cl-letf (((symbol-function 'auto-complete-nxml-get-candidates)
                    (lambda () nil)))
           (with-temp-buffer
             (insert
              "<button class=\"primary wide\">Save</button>"
              "<button class=\"secondary compact\">Cancel</button>"
              "<button class=\"pri")
             (goto-char (point-max))
             (let ((context
                    (auto-complete-nxml-get-current-context-symbol))
                   (candidates
                    (auto-complete-nxml-get-attr-value-candidates)))
               (list
                context
                auto-complete-nxml-buffer-current-attr
                candidates
                (member "primary" candidates)
                (member "secondary" candidates))))))"##,
        expect![[
            r#"OK (attrvalue "class" ("compact" . #2=("secondary" "wide" . #1=("primary"))) #1# #2#)"#
        ]],
    )
}

fn auto_complete_nxml_document_capture_and_popup_render_end_to_end() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_document_capture_and_popup_render_end_to_end",
        r##"(progn
         (auto-complete-nxml-start-make-doc4ac-in-nxml)
         (puthash 0 '(name (nil . "section"))
                  auto-complete-nxml-ncls-store-hash)
         (puthash 0 "Groups related content."
                  auto-complete-nxml-note-store-hash)
         (auto-complete-nxml-make-document
          0 0 "Structural element."
          auto-complete-nxml-element-document-hash)
         (cl-letf (((symbol-function 'nxml-ns-get-default)
                    (lambda () nil)))
           (list
            (acnxml-test-doc-value
             (gethash "section"
                      auto-complete-nxml-element-document-hash))
            (auto-complete-nxml-get-document-tag "section"))))"##,
        expect![[
            r#"OK ((:name "section" :ns "" :comment "Structural element." :note "Groups related content.") "'section' is ELEMENT in ''.\n\nComment: \nStructural element.\n\nNote: \nGroups related content.\n")"#
        ]],
    )
}

fn auto_complete_nxml_source_actions_build_a_complete_attribute_and_element() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_source_actions_build_a_complete_attribute_and_element",
        r##"(let ((auto-complete-nxml-automatic-p nil))
         (cl-letf (((symbol-function 'auto-complete-nxml-expand-tag)
                    (lambda () (insert " "))))
           (with-temp-buffer
             (insert "<article")
             (funcall (cdr (assq 'action ac-source-nxml-tag)))
             (insert "class")
             (funcall (cdr (assq 'action ac-source-nxml-attr)))
             (insert "featured")
             (goto-char (point-max))
             (insert ">Body")
             (funcall
              (cdr
               (assq 'action ac-source-nxml-tag-value-by-nxml)))
             (list (buffer-string) (point)))))"##,
        expect![[r#"OK ("<article class=\"featured\">Body</article>" 41)"#]],
    )
}

fn auto_complete_nxml_popup_help_routes_context_document_to_popup_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_popup_help_routes_context_document_to_popup_backend",
        r##"(let ((auto-complete-nxml-element-document-hash
                                (make-hash-table :test 'equal))
             calls)
         (puthash
          "item"
          (make-auto-complete-nxml-doc
           :name "item"
           :ns ""
           :comment "An item."
           :note "Practical note.")
          auto-complete-nxml-element-document-hash)
         (cl-letf (((symbol-function 'nxml-ns-get-default) (lambda () nil))
                   ((symbol-function 'ac-quick-help-use-pos-tip-p)
                    (lambda () nil))
                   ((symbol-function 'popup-tip)
                    (lambda (document)
                      (push document calls)
                      :shown)))
           (with-temp-buffer
             (insert "<item")
             (auto-complete-nxml-popup-help)
             (list
              auto-complete-nxml-buffer-current-tag
              (nreverse calls)))))"##,
        expect![[r#"OK ("<" ("'<' is ELEMENT in ''.\n\nNot documented.\n"))"#]],
    )
}

fn auto_complete_nxml_project_initialization_indexes_deterministic_xml_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_project_initialization_indexes_deterministic_xml_files",
        r##"(let* ((root
                                 (expand-file-name
                                  "auto-complete-nxml-project"
                                  default-directory))
              (first (expand-file-name "first.xml" root))
              (second (expand-file-name "second.xml" root))
              (auto-complete-nxml-tag-value-words-hash
               (make-hash-table :test 'equal))
              (auto-complete-nxml-attr-words-hash-hash
               (make-hash-table :test 'equal))
              (auto-mode-alist '(("\\.xml\\'" . nxml-mode)))
              (ac-prefix ""))
         (when (file-exists-p root)
           (delete-directory root t))
         (unwind-protect
             (progn
               (make-directory root t)
               (with-temp-file first
                 (insert "<item class=\"primary\">Alpha text</item>"))
               (with-temp-file second
                 (insert "<item class=\"secondary\">Beta text</item>"))
               (provide 'anything-project)
               (cl-letf (((symbol-function 'ap:get-root-directory)
                          (lambda () root))
                         ((symbol-function 'ap:get-project-files)
                          (lambda () '("first.xml" "second.xml")))
                         ((symbol-function 'ap:expand-file)
                          (lambda (file)
                            (expand-file-name file root))))
                 (auto-complete-nxml-init-project)
                 (list
                  (auto-complete-nxml-get-project-tag-value-words root)
                  (acnxml-test-hash-alist
                   (auto-complete-nxml-get-project-attr-words-hash root))
                  (sort
                   (mapcar
                    (lambda (file)
                      (file-relative-name file root))
                    (directory-files root t "\\.xml\\'"))
                   #'string<))))
           (when (file-exists-p root)
             (delete-directory root t))))"##,
        expect![[
            r#"OK (("Beta" "text" "Alpha") (("class" "secondary" "primary")) ("first.xml" "second.xml"))"#
        ]],
    )
}

fn auto_complete_nxml_namespace_completion_and_document_lookup_share_prefix_mapping()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_namespace_completion_and_document_lookup_share_prefix_mapping",
        r##"(let ((auto-complete-nxml-element-document-hash
                                (make-hash-table :test 'equal)))
         (puthash
          "urn:math:sum"
          (make-auto-complete-nxml-doc
           :name "sum" :ns "urn:math"
           :comment "Summation." :note "MathML.")
          auto-complete-nxml-element-document-hash)
         (cl-letf (((symbol-function 'rng-match-possible-namespace-uris)
                    (lambda () '(default-ns math-ns)))
                   ((symbol-function 'nxml-namespace-name)
                    (lambda (symbol)
                      (if (eq symbol 'default-ns)
                          "urn:html"
                        "urn:math")))
                   ((symbol-function 'auto-complete-nxml-get-prefix)
                    (lambda (namespace)
                      (and (equal namespace "urn:math") "m")))
                   ((symbol-function 'nxml-ns-get-prefix)
                    (lambda (prefix)
                      (and (equal prefix "m") 'math-ns))))
           (with-temp-buffer
             (insert "<html xmlns=\"urn:html")
             (auto-complete-nxml-expand-other-xmlns)
             (list
              (buffer-string)
              (auto-complete-nxml-get-document-tag "m:sum")))))"##,
        expect![[
            r#"OK ("<html xmlns=\"urn:html\"\n      xmlns:m=\"urn:math\"" "'sum' is ELEMENT in 'urn:math'.\n\nComment: \nSummation.\n\nNote: \nMathML.\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_nxml_real_nxml_mode_setup_installs_completion_environment(),
        auto_complete_nxml_practical_content_completion_reuses_words_from_open_document(),
        auto_complete_nxml_practical_attribute_completion_reuses_matching_attribute_values(),
        auto_complete_nxml_document_capture_and_popup_render_end_to_end(),
        auto_complete_nxml_source_actions_build_a_complete_attribute_and_element(),
        auto_complete_nxml_popup_help_routes_context_document_to_popup_backend(),
        auto_complete_nxml_project_initialization_indexes_deterministic_xml_files(),
        auto_complete_nxml_namespace_completion_and_document_lookup_share_prefix_mapping(),
    ]
}
