use expect_test::expect;

use super::ParityBatchCase;

fn sentence_navigation_resolves_the_enclosing_and_next_asciidoc_things() -> ParityBatchCase {
    ParityBatchCase::value(
        "sentence_navigation_resolves_the_enclosing_and_next_asciidoc_things",
        r##"(with-temp-buffer
  (insert
   "= Handbook\n\n"
   "Opening sentence. Second sentence.\n\n"
   "== Install\n\n"
   "Install prose.\n\n"
   "=== Linux\n\n")
  (asciidoc-mode)
  (cl-labels
      ((summary
        (node)
        (and node
             (list (treesit-node-type node)
                   (treesit-node-start node)
                   (treesit-node-end node)
                   (treesit-node-match-p node 'text t)
                   (treesit-node-match-p node 'sentence t)))))
    (list
     (summary (treesit-node-at 48 'asciidoc))
     (summary (treesit-thing-at 48 'sentence))
     (summary (treesit-thing-next 48 'sentence 'asciidoc))
     (save-excursion
       (goto-char 48)
       (list (treesit-end-of-thing 'sentence 1) (point)))
     (save-excursion
       (goto-char 48)
       (forward-sentence)
       (point)))))"##,
        expect![[r#"OK (("line" 13 48 nil nil) nil ("paragraph" 61 76 nil t) (76 76) 76)"#]],
    )
}

fn imenu_defun_sentence_and_outline_navigation_traverse_a_real_document_hierarchy()
-> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_defun_sentence_and_outline_navigation_traverse_a_real_document_hierarchy",
        r##"(with-temp-buffer
  (insert
   "= Handbook\n\n"
   "Opening sentence. Second sentence.\n\n"
   "== Install\n\n"
   "Install prose.\n\n"
   "=== Linux\n\n"
   "Linux prose.\n\n"
   "=== macOS\n\n"
   "macOS prose.\n\n"
   "== Operate\n\n"
   "Operate prose.\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (let* ((index (treesit-simple-imenu))
         (sections (cdr (assoc "Section" index)))
         forward
         backward
         sentence)
    (goto-char (point-min))
    (dotimes (_ 4)
      (beginning-of-defun -1)
      (push
       (buffer-substring-no-properties
        (line-beginning-position)
        (line-end-position))
       forward))
    (goto-char (point-max))
    (dotimes (_ 4)
      (beginning-of-defun)
      (push
       (buffer-substring-no-properties
        (line-beginning-position)
        (line-end-position))
       backward))
    (goto-char (point-min))
    (search-forward "Opening")
    (goto-char (match-beginning 0))
    (dotimes (_ 2)
      (forward-sentence)
      (push
       (list
        (point)
        (buffer-substring-no-properties
         (line-beginning-position)
         (line-end-position)))
       sentence))
    (list
     (mapcar
      (lambda (item)
        (list
         (car item)
         (marker-position (cdr item))))
      sections)
     (nreverse forward)
     backward
     (nreverse sentence)
     (funcall
      treesit-defun-name-function
      (treesit-node-at
       (save-excursion
         (goto-char (point-min))
         (search-forward "== Install")
         (match-beginning 0))
       'asciidoc)))))"##,
        expect![[
            r#"OK ((("Install\n" 49) ("Linux\n" 77) ("macOS\n" 102) ("Operate\n" 127)) ("== Install" "=== Linux" "=== macOS" "== Operate") ("== Install" "=== Linux" "=== macOS" "== Operate") ((48 "") (76 "")) "")"#
        ]],
    )
}

fn outline_subtree_move_reports_the_exact_editor_error_without_mutating_the_document()
-> ParityBatchCase {
    ParityBatchCase::value(
        "outline_subtree_move_reports_the_exact_editor_error_without_mutating_the_document",
        r##"(with-temp-buffer
  (insert
   "= Runbook\n\n"
   "== Prepare\n\n"
   "Preparation body.\n\n"
   "=== Verify\n\n"
   "Verification body.\n\n"
   "== Deploy\n\n"
   "Deployment body.\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (goto-char (point-min))
  (search-forward "== Prepare")
  (beginning-of-line)
  (let ((before (buffer-string)))
    (condition-case down-error
        (progn
          (outline-move-subtree-down)
          (let ((after-down (buffer-string))
                (down-point (point)))
            (condition-case up-error
                (progn
                  (outline-move-subtree-up)
                  (list
                   'moved
                   before
                   after-down
                   down-point
                   (buffer-string)
                   (equal before (buffer-string))
                   (buffer-substring-no-properties
                    (line-beginning-position)
                    (line-end-position))))
              (error
               (list
                'up-error
                (car up-error)
                (cdr up-error)
                before
                after-down
                (buffer-string))))))
      (error
       (list
        'down-error
        (car down-error)
        (cdr down-error)
        before
        (buffer-string))))))"##,
        expect![[
            r#"OK (down-error wrong-type-argument (number-or-marker-p nil) #("= Runbook\n\n== Prepare\n\nPreparation body.\n\n=== Verify\n\nVerification body.\n\n== Deploy\n\nDeployment body.\n" 0 1 (face asciidoc-document-title-face) 2 10 (face asciidoc-document-title-face) 11 22 (face asciidoc-title-1-face) 42 53 (face asciidoc-title-2-face) 74 84 (face asciidoc-title-1-face)) #("= Runbook\n\n== Prepare\n\nPreparation body.\n\n=== Verify\n\nVerification body.\n\n== Deploy\n\nDeployment body.\n" 0 1 (face asciidoc-document-title-face) 2 10 (face asciidoc-document-title-face) 11 22 (face asciidoc-title-1-face) 42 53 (face asciidoc-title-2-face) 74 84 (face asciidoc-title-1-face)))"#
        ]],
    )
}

fn public_follow_command_jumps_to_anchors_and_routes_supported_urls_and_macros() -> ParityBatchCase
{
    ParityBatchCase::value(
        "public_follow_command_jumps_to_anchors_and_routes_supported_urls_and_macros",
        r##"(with-temp-buffer
  (insert
   "[[target]] Explicit destination.\n\n"
   "== Natural Section\n\n"
   "See <<target>> and <<_natural_section>>.\n"
   "Visit https://example.com/path.\n"
   "See link:https://docs.example.org[docs].\n"
   "Mail mailto:ada@example.org[Ada].\n"
   "Image image:diagram.svg[Diagram].\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (let (opened results)
    (cl-letf
        (((symbol-function 'browse-url)
          (lambda (url &rest arguments)
            (push (cons url arguments) opened)
            'opened))
         ((symbol-function
           'pulse-momentary-highlight-one-line)
          (lambda (&rest _arguments)
            'highlighted)))
      (dolist
          (case
           '(("<<target>>" 2 anchor)
             ("<<_natural_section>>" 2 section)))
        (goto-char (point-min))
        (search-forward (nth 0 case))
        (goto-char
         (+ (match-beginning 0)
            (nth 1 case)))
        (let ((from (point)))
          (asciidoc-follow-reference-at-point)
          (push
           (list
            (nth 2 case)
            from
            (point)
            (mark)
            (buffer-substring-no-properties
             (line-beginning-position)
             (line-end-position)))
           results)))
      (dolist
          (case
           '(("https://example.com/path" 3)
             ("link:https://docs.example.org" 7)
             ("mailto:ada@example.org" 8)))
        (goto-char (point-min))
        (search-forward (nth 0 case))
        (goto-char
         (+ (match-beginning 0)
            (nth 1 case)))
        (push
         (asciidoc-follow-reference-at-point)
         results))
      (goto-char (point-min))
      (search-forward "image:diagram")
      (goto-char (match-beginning 0))
      (let ((image-error
             (condition-case error
                 (asciidoc-follow-reference-at-point)
               (error
                (list (car error)
                      (cdr error))))))
        (goto-char (point-max))
        (let ((plain-error
               (condition-case error
                   (asciidoc-follow-reference-at-point)
                 (error
                  (list (car error)
                        (cdr error))))))
          (list
           (nreverse results)
           (nreverse opened)
           image-error
           plain-error))))))"##,
        expect![[
            r#"OK (((anchor 61 1 61 "[[target]] Explicit destination.") (section 76 35 76 "== Natural Section") opened opened opened) (("https://example.com/path.\nSee") ("https://docs.example.org") ("mailto:ada@example.org")) (user-error ("Nothing to follow at point")) (user-error ("No reference at point")))"#
        ]],
    )
}

fn xref_backend_identifies_defines_completes_and_lists_real_reference_usages() -> ParityBatchCase {
    ParityBatchCase::value(
        "xref_backend_identifies_defines_completes_and_lists_real_reference_usages",
        r##"(with-temp-buffer
  (insert
   "= Manual\n\n"
   "== Getting Started\n\n"
   "[[explicit]] Definition.\n\n"
   "See <<explicit>>, xref:explicit[again], <<explicit,caption>>, "
   "and <<_getting_started>>.\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (cl-labels
      ((identifier-at
        (needle offset)
        (goto-char (point-min))
        (search-forward needle)
        (goto-char
         (+ (match-beginning 0) offset))
        (xref-backend-identifier-at-point
         'asciidoc))
       (definition
        (id)
        (let* ((items
                (xref-backend-definitions
                 'asciidoc id))
               (item (car items))
               (location
                (and item
                     (xref-item-location item)))
               (marker
                (and location
                     (xref-location-marker
                      location))))
          (list
           (length items)
           (and marker
                (marker-position marker))
           (and marker
                (with-current-buffer
                    (marker-buffer marker)
                  (save-excursion
                    (goto-char marker)
                    (buffer-substring-no-properties
                     (line-beginning-position)
                     (line-end-position)))))))))
    (list
     (asciidoc--xref-backend)
     (identifier-at "<<explicit>>" 3)
     (identifier-at "xref:explicit" 6)
     (identifier-at "[[explicit]]" 3)
     (definition "explicit")
     (definition "_getting_started")
     (definition "Getting Started")
     (xref-backend-identifier-completion-table
      'asciidoc)
     (mapcar
      (lambda (item)
        (list
         (xref-item-summary item)
         (marker-position
          (xref-location-marker
           (xref-item-location item)))))
      (xref-backend-references
       'asciidoc "explicit")))))"##,
        expect![[
            r#"OK (asciidoc "explicit" "explicit" "explicit" (1 31 "[[explicit]] Definition.") (1 11 "== Getting Started") (1 11 "== Getting Started") ("explicit" "_manual" "Manual" "_getting_started" "Getting Started") (("See <<explicit>>, xref:explicit[again], <<explicit,caption>>, and <<_getting_started>>." 61) ("See <<explicit>>, xref:explicit[again], <<explicit,caption>>, and <<_getting_started>>." 97) ("See <<explicit>>, xref:explicit[again], <<explicit,caption>>, and <<_getting_started>>." 75)))"#
        ]],
    )
}

fn section_and_anchor_algorithms_cover_defaults_custom_attributes_and_precedence() -> ParityBatchCase
{
    ParityBatchCase::value(
        "section_and_anchor_algorithms_cover_defaults_custom_attributes_and_precedence",
        r##"(list
 (with-temp-buffer
   (list
    (asciidoc--section-id
     "Introduction to AsciiDoc")
    (asciidoc--section-id "What's New?")
    (asciidoc--section-id "Section 1.2")
    (asciidoc--section-id
     "  Multiple --- Separators  ")
    (asciidoc--section-id
     "Already Clean" "sect-" "-")))
 (with-temp-buffer
   (insert
    ":idprefix: sect_\n"
    ":idseparator: -\n"
    ":empty:\n\n"
    "[[_hello-world]] explicit wins.\n\n"
    "== Hello World\n\n"
    "[#short.role] shorthand.\n")
   (list
    (asciidoc--doc-attribute
     "idprefix" "missing")
    (asciidoc--doc-attribute
     "empty" "fallback")
    (asciidoc--doc-attribute
     "absent" "fallback")
    (asciidoc--id-prefix-separator)
    (asciidoc--section-id "Hello World")
    (asciidoc--explicit-anchor-position
     "_hello-world")
    (asciidoc--section-anchor-position
     "sect_hello-world")
    (asciidoc--anchor-position
     "_hello-world")
    (asciidoc--anchor-position "short")
    (asciidoc--anchor-position
     "Hello World")
    (asciidoc--all-anchor-ids))))"##,
        expect![[
            r#"OK (("_introduction_to_asciidoc" "_what_s_new" "_section_1_2" "_multiple_separators" "sect-already-clean") ("sect_" "" "fallback" ("sect_" . "-") "sect_hello-world" 43 76 43 92 76 ("_hello-world" "short" "sect_hello-world" "Hello World")))"#
        ]],
    )
}

fn generic_and_antora_cross_file_resolution_use_deterministic_real_project_layouts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "generic_and_antora_cross_file_resolution_use_deterministic_real_project_layouts",
        r##"(let* ((root
         (expand-file-name
          "asciidoc-mode-reference-contract"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (generic
        (expand-file-name "generic" root))
       (main
        (expand-file-name "main.adoc" generic))
       (target
        (expand-file-name
         "guide/target.adoc" generic))
       (antora
        (expand-file-name "antora" root))
       (pages
        (expand-file-name
         "modules/ROOT/pages" antora))
       (usage
        (expand-file-name "usage.adoc" pages))
       (install
        (expand-file-name
         "basics/install.adoc" pages))
       opened)
  (when (file-directory-p root)
    (delete-directory root t))
  (make-directory
   (file-name-directory target) t)
  (make-directory
   (file-name-directory install) t)
  (with-temp-file main
    (insert
     "= Main\n\n"
     "See xref:guide/target.adoc#anchor[].\n"))
  (with-temp-file target
    (insert
     "= Target\n\n"
     "[[anchor]] destination.\n"))
  (with-temp-file
      (expand-file-name "antora.yml" antora)
    (insert
     "name: practical-docs\n"
     "version: ~\n"))
  (with-temp-file usage
    (insert
     "= Usage\n\n"
     "See xref:basics/install.adoc#setup-steps[].\n"))
  (with-temp-file install
    (insert
     "= Install\n\n"
     "== Setup Steps\n\n"
     "content\n"))
  (unwind-protect
      (let (generic-result antora-result)
        (with-current-buffer
            (find-file-noselect main)
          (push (current-buffer) opened)
          (let ((marker
                 (asciidoc--resolve-reference
                  "guide/target.adoc#anchor")))
            (push (marker-buffer marker) opened)
            (setq
             generic-result
             (list
              (file-relative-name
               (buffer-file-name
                (marker-buffer marker))
               root)
              (marker-position marker)
              (with-current-buffer
                  (marker-buffer marker)
                (save-excursion
                  (goto-char marker)
                  (buffer-substring-no-properties
                   (line-beginning-position)
                   (line-end-position))))
              (asciidoc--reference-target-file
               "guide/target.adoc#anchor")
              (asciidoc--reference-target-file
               "local-anchor")))))
        (with-current-buffer
            (find-file-noselect usage)
          (push (current-buffer) opened)
          (let* ((context
                  (asciidoc--antora-context))
                 (marker
                  (asciidoc--resolve-reference
                   "basics/install.adoc#setup-steps"))
                 (module-marker
                  (asciidoc--resolve-reference
                   "ROOT:basics/install#setup-steps"))
                 (version-marker
                  (asciidoc--resolve-reference
                   "2.0@ROOT:basics/install.adoc#setup-steps"))
                 (foreign
                  (asciidoc--resolve-reference
                   "other:ROOT:basics/install.adoc")))
            (dolist (value
                     (list marker module-marker
                           version-marker))
              (push (marker-buffer value) opened))
            (setq
             antora-result
             (list
              (list
               (file-relative-name
                (plist-get context :root)
                root)
               (plist-get context :module)
               (plist-get context :component))
              (mapcar
               (lambda (value)
                 (list
                  (file-relative-name
                   (buffer-file-name
                    (marker-buffer value))
                   root)
                  (marker-position value)))
               (list marker module-marker
                     version-marker))
              foreign
              (asciidoc--id-prefix-separator)))))
        (list generic-result antora-result))
    (dolist (buffer (delete-dups opened))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))
    (delete-directory root t)))"##,
        expect![[
            r#"OK (("generic/guide/target.adoc" 11 "[[anchor]] destination." "guide/target.adoc" nil) (("antora" "ROOT" "practical-docs") (("antora/modules/ROOT/pages/basics/install.adoc" 12) ("antora/modules/ROOT/pages/basics/install.adoc" 12) ("antora/modules/ROOT/pages/basics/install.adoc" 12)) nil ("" . "-")))"#
        ]],
    )
}

pub(super) fn navigation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        sentence_navigation_resolves_the_enclosing_and_next_asciidoc_things(),
        imenu_defun_sentence_and_outline_navigation_traverse_a_real_document_hierarchy(),
        outline_subtree_move_reports_the_exact_editor_error_without_mutating_the_document(),
        public_follow_command_jumps_to_anchors_and_routes_supported_urls_and_macros(),
        xref_backend_identifies_defines_completes_and_lists_real_reference_usages(),
        section_and_anchor_algorithms_cover_defaults_custom_attributes_and_precedence(),
        generic_and_antora_cross_file_resolution_use_deterministic_real_project_layouts(),
    ]
}
