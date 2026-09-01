use expect_test::expect;

use super::ParityBatchCase;

fn construct_dense_document_applies_exact_semantic_faces_and_interaction_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "construct_dense_document_applies_exact_semantic_faces_and_interaction_properties",
        r##"(with-temp-buffer
  (insert
   "= Practical AsciiDoc\n"
   ":author: Ada Lovelace\n\n"
   "== Semantics\n\n"
   "Text with *bold*, _italic_, `code`, #marked#, E=mc^2^, and H~2~O.\n"
   "A [.underline]#styled# span and \"`quoted prose`\".\n"
   "See https://example.com[the docs], <<target>>, and footnote:[a source].\n\n"
   "[[target]]\n"
   "CPU:: The *brain* of the system.\n"
   "* [x] deployed\n\n"
   "NOTE: Review `code` carefully.\n"
   "This continuation is important.\n\n"
   "[source,emacs-lisp]\n"
   "----\n"
   "(defun practical-demo () t)\n"
   "----\n\n"
   "|===\n"
   "a| cell\n"
   "|===\n\n"
   "toc::[]\n"
   "// final comment\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (let ((case-fold-search nil)
        (probe
         (lambda (label needle offset)
           (save-excursion
             (goto-char (point-min))
             (search-forward needle)
             (let* ((position
                     (+ (match-beginning 0) offset))
                    (face
                     (get-text-property
                      position 'face))
                    (keymap
                     (get-text-property
                      position 'keymap)))
               (list
                label
                position
                face
                (get-text-property
                 position 'display)
                (eq keymap
                    asciidoc-reference-map)
                (get-text-property
                 position 'mouse-face)
                (get-text-property
                 position 'follow-link)
                (get-text-property
                 position 'font-lock-multiline)))))))
    (prin1-to-string
     (mapcar
      (lambda (spec)
        (funcall probe
                 (nth 0 spec)
                 (nth 1 spec)
                 (nth 2 spec)))
      '(("document-title" "= Practical AsciiDoc" 0)
        ("attribute-key" ":author:" 1)
        ("attribute-value" "Ada Lovelace" 0)
        ("section-title" "== Semantics" 0)
        ("bold" "*bold*" 1)
        ("italic" "_italic_" 1)
        ("code" "`code`" 1)
        ("highlight" "#marked#" 1)
        ("superscript" "mc^2^" 3)
        ("subscript" "H~2~O" 2)
        ("role" "underline" 0)
        ("role-text" "styled" 0)
        ("quote-marker" "\"`quoted" 0)
        ("quote-text" "quoted prose" 0)
        ("url" "https://example.com" 0)
        ("link-label" "the docs" 0)
        ("xref" "<<target>>" 2)
        ("footnote-marker" "footnote:" 0)
        ("footnote-body" "a source" 0)
        ("anchor" "[[target]]" 2)
        ("description-term" "CPU::" 0)
        ("description-marker" "CPU::" 3)
        ("list-marker" "* [x]" 0)
        ("checkbox" "[x]" 0)
        ("admonition-label" "NOTE:" 0)
        ("admonition-body" "Review" 0)
        ("admonition-continuation" "This continuation" 0)
        ("source-attribute" "[source,emacs-lisp]" 1)
        ("listing-fence" "----" 0)
        ("native-keyword" "defun" 0)
        ("table-fence" "|===" 0)
        ("cell-specifier" "a| cell" 0)
        ("block-macro" "toc::" 0)
        ("comment" "// final comment" 0))))))"##,
        expect![[
            r#"OK "((\"document-title\" 1 asciidoc-document-title-face nil nil nil nil nil) (\"attribute-key\" 23 asciidoc-metadata-key-face nil nil nil nil nil) (\"attribute-value\" 31 asciidoc-metadata-value-face nil nil nil nil nil) (\"section-title\" 45 asciidoc-title-1-face nil nil nil nil nil) (\"bold\" 70 bold nil nil nil nil nil) (\"italic\" 78 italic nil nil nil nil nil) (\"code\" 88 asciidoc-code-face nil nil nil nil nil) (\"highlight\" 96 asciidoc-highlight-face nil nil nil nil nil) (\"superscript\" 110 asciidoc-superscript-face (raise 0.4) nil nil nil nil) (\"subscript\" 120 asciidoc-subscript-face (raise -0.25) nil nil nil nil) (\"role\" 129 font-lock-preprocessor-face nil nil nil nil nil) (\"role-text\" 140 asciidoc-underline-face nil nil nil nil nil) (\"quote-marker\" 157 asciidoc-markup-face nil nil nil nil nil) (\"quote-text\" 159 nil nil nil nil nil nil) (\"url\" 179 asciidoc-url-face nil t asciidoc-link-mouse-face t nil) (\"link-label\" 199 asciidoc-link-face nil t asciidoc-link-mouse-face t nil) (\"xref\" 212 asciidoc-cross-reference-face nil t asciidoc-link-mouse-face t nil) (\"footnote-marker\" 226 asciidoc-footnote-marker-face nil nil nil nil nil) (\"footnote-body\" 236 asciidoc-footnote-text-face nil nil nil nil nil) (\"anchor\" 250 asciidoc-anchor-face nil nil nil nil nil) (\"description-term\" 259 font-lock-keyword-face nil nil nil nil nil) (\"description-marker\" 262 asciidoc-markup-face nil nil nil nil nil) (\"list-marker\" 292 asciidoc-markup-face nil nil nil nil nil) (\"checkbox\" 294 font-lock-constant-face nil nil nil nil nil) (\"admonition-label\" 308 (asciidoc-admonition-note-label-face . #1=(asciidoc-admonition-note-face)) nil nil nil nil t) (\"admonition-body\" 314 #1# nil nil nil nil t) (\"admonition-continuation\" 339 (asciidoc-admonition-note-face) nil nil nil nil t) (\"source-attribute\" 373 font-lock-preprocessor-face nil nil nil nil nil) (\"listing-fence\" 392 asciidoc-markup-face nil nil nil nil nil) (\"native-keyword\" 398 font-lock-keyword-face nil nil nil nil nil) (\"table-fence\" 431 asciidoc-markup-face nil nil nil nil nil) (\"cell-specifier\" 436 font-lock-preprocessor-face nil nil nil nil nil) (\"block-macro\" 450 font-lock-function-call-face nil nil nil nil nil) (\"comment\" 458 font-lock-comment-face nil nil nil nil nil))""#
        ]],
    )
}

fn fontification_is_idempotent_across_flush_and_reparse_for_a_mixed_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontification_is_idempotent_across_flush_and_reparse_for_a_mixed_document",
        r##"(with-temp-buffer
  (insert
   "= Stable Document\n"
   ":toc: left\n\n"
   "== Lists and Links\n\n"
   "* item with `code`\n"
   "** nested *bold* item\n\n"
   "NOTE: Visit https://example.com[docs].\n"
   "Continuation with [.underline]#styled# text.\n\n"
   "[source,emacs-lisp]\n"
   "----\n"
   "(defun stable () 'yes)\n"
   "----\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (cl-labels
      ((properties
        ()
        (let ((position (point-min))
              result)
          (while (< position (point-max))
            (push
             (list
              position
              (get-text-property position 'face)
              (get-text-property position 'display)
              (eq
               (get-text-property position 'keymap)
               asciidoc-reference-map)
              (get-text-property
               position 'mouse-face)
              (get-text-property
               position 'follow-link)
              (get-text-property
               position 'font-lock-multiline))
             result)
            (setq position (1+ position)))
          (nreverse result))))
    (let ((before (properties))
          (block-before
           (treesit-node-string
            (treesit-buffer-root-node
             'asciidoc)))
          (inline-before
           (treesit-node-string
            (treesit-buffer-root-node
             'asciidoc-inline))))
      (font-lock-flush)
      (font-lock-ensure)
      (let ((after (properties)))
        (list
         (= (length before)
            (length after))
         (equal before after)
         (equal
          block-before
          (treesit-node-string
           (treesit-buffer-root-node
            'asciidoc)))
         (equal
          inline-before
          (treesit-node-string
           (treesit-buffer-root-node
            'asciidoc-inline)))
         (length
          (delete-dups
           (delq nil
                 (mapcar #'cadr after)))))))))"##,
        expect!["OK (t t t t 16)"],
    )
}

fn source_language_extraction_and_mode_resolution_cover_options_aliases_and_fallbacks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "source_language_extraction_and_mode_resolution_cover_options_aliases_and_fallbacks",
        r##"(list
 (mapcar
  (lambda (value)
    (cons value
          (asciidoc--code-block-language value)))
  '("source,ruby"
    ",js"
    "source%nowrap,python"
    " source , emacs-lisp "
    "source,lang,linenums"
    "source,language=rust"
    "NOTE"
    "quote,ruby"
    "source"
    ""))
 (let ((asciidoc-code-lang-modes
        '(("direct" . emacs-lisp-mode)
          ("candidates"
           . (asciidoc-no-such-mode
              emacs-lisp-mode))
          ("missing"
           . (asciidoc-no-such-mode)))))
   (mapcar
    (lambda (language)
      (cons
       language
       (asciidoc--code-block-lang-mode
        language)))
    '("direct"
      "DIRECT"
      "candidates"
      "emacs-lisp"
      "json"
      "missing")))
 (let ((asciidoc-code-lang-modes
        '(("mapped" . emacs-lisp-mode)))
       (major-mode-remap-alist
        '((emacs-lisp-mode
           . lisp-interaction-mode))))
   (asciidoc--code-block-lang-mode
    "mapped")))"##,
        expect![[
            r#"OK ((("source,ruby" . "ruby") (",js" . "js") ("source%nowrap,python" . "python") (" source , emacs-lisp " . "emacs-lisp") ("source,lang,linenums" . "lang") ("source,language=rust") ("NOTE") ("quote,ruby") ("source") ("")) (("direct" . emacs-lisp-mode) ("DIRECT" . emacs-lisp-mode) ("candidates" . emacs-lisp-mode) ("emacs-lisp" . emacs-lisp-mode) ("json") ("missing")) lisp-interaction-mode)"#
        ]],
    )
}

fn native_source_fontification_honors_enablement_size_language_and_recursion_guards()
-> ParityBatchCase {
    ParityBatchCase::value(
        "native_source_fontification_honors_enablement_size_language_and_recursion_guards",
        r##"(cl-labels
    ((inspect
      (setting attribute body needle)
      (let ((asciidoc-fontify-code-blocks-natively
             setting))
        (with-temp-buffer
          (insert
           "= Source Matrix\n\n"
           attribute "\n"
           "----\n"
           body "\n"
           "----\n")
          (asciidoc-mode)
          (font-lock-ensure)
          (goto-char (point-min))
          (search-forward needle)
          (list
           (get-text-property
            (match-beginning 0) 'face)
           (buffer-string))))))
  (list
   (inspect
    5000 "[source,emacs-lisp]"
    "(defun demo () nil)" "defun")
   (inspect
    nil "[source,emacs-lisp]"
    "(defun demo () nil)" "defun")
   (inspect
    3 "[source,emacs-lisp]"
    "(defun demo () nil)" "defun")
   (inspect
    t "[source,nosuchlang]"
    "plain body" "plain")
   (inspect
    t "" "(defun demo () nil)" "defun")
   (inspect
    t "[source,asciidoc]"
    "== Nested" "Nested")))"##,
        expect![[
            r#"OK ((font-lock-keyword-face #("= Source Matrix\n\n[source,emacs-lisp]\n----\n(defun demo () nil)\n----\n" 0 1 (face asciidoc-document-title-face) 2 16 (face asciidoc-document-title-face) 17 37 (face font-lock-preprocessor-face) 37 41 (face asciidoc-markup-face) 42 43 (face nil) 43 48 (face font-lock-keyword-face) 48 49 (face nil) 49 53 (face font-lock-function-name-face) 53 62 (face nil) 62 66 (face asciidoc-markup-face))) (asciidoc-code-face #("= Source Matrix\n\n[source,emacs-lisp]\n----\n(defun demo () nil)\n----\n" 0 1 (face asciidoc-document-title-face) 2 16 (face asciidoc-document-title-face) 17 37 (face font-lock-preprocessor-face) 37 41 (face asciidoc-markup-face) 42 62 (face asciidoc-code-face) 62 66 (face asciidoc-markup-face))) (asciidoc-code-face #("= Source Matrix\n\n[source,emacs-lisp]\n----\n(defun demo () nil)\n----\n" 0 1 (face asciidoc-document-title-face) 2 16 (face asciidoc-document-title-face) 17 37 (face font-lock-preprocessor-face) 37 41 (face asciidoc-markup-face) 42 62 (face asciidoc-code-face) 62 66 (face asciidoc-markup-face))) (asciidoc-code-face #("= Source Matrix\n\n[source,nosuchlang]\n----\nplain body\n----\n" 0 1 (face asciidoc-document-title-face) 2 16 (face asciidoc-document-title-face) 17 37 (face font-lock-preprocessor-face) 37 41 (face asciidoc-markup-face) 42 53 (face asciidoc-code-face) 53 57 (face asciidoc-markup-face))) (asciidoc-code-face #("= Source Matrix\n\n\n----\n(defun demo () nil)\n----\n" 0 1 (face asciidoc-document-title-face) 2 16 (face asciidoc-document-title-face) 18 22 (face asciidoc-markup-face) 23 43 (face asciidoc-code-face) 43 47 (face asciidoc-markup-face))) (asciidoc-code-face #("= Source Matrix\n\n[source,asciidoc]\n----\n== Nested\n----\n" 0 1 (face asciidoc-document-title-face) 2 16 (face asciidoc-document-title-face) 17 35 (face font-lock-preprocessor-face) 35 39 (face asciidoc-markup-face) 40 50 (face asciidoc-code-face) 50 54 (face asciidoc-markup-face))))"#
        ]],
    )
}

fn editing_an_admonition_clears_multiline_background_and_preserves_inline_faces() -> ParityBatchCase
{
    ParityBatchCase::value(
        "editing_an_admonition_clears_multiline_background_and_preserves_inline_faces",
        r##"(with-temp-buffer
  (insert
   "= Editing\n\n"
   "NOTE: inspect `code` first.\n"
   "Continue on this line.\n\n"
   "Plain paragraph.\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (cl-labels
      ((faces-at
        (needle)
        (save-excursion
          (goto-char (point-min))
          (search-forward needle)
          (let ((position
                 (match-beginning 0)))
            (list
             (get-text-property position 'face)
             (get-text-property
              position 'font-lock-multiline))))))
    (let ((before
           (list
            (faces-at "NOTE:")
            (faces-at "code")
            (faces-at "Continue")
            (faces-at "Plain"))))
      (goto-char (point-min))
      (search-forward "NOTE")
      (replace-match "TEXT")
      (font-lock-flush)
      (font-lock-ensure)
      (list
       before
       (list
        (faces-at "TEXT:")
        (faces-at "code")
        (faces-at "Continue")
        (faces-at "Plain"))
       (buffer-string)))))"##,
        expect![[
            r#"OK ((((asciidoc-admonition-note-label-face asciidoc-admonition-note-face) t) ((asciidoc-admonition-note-face asciidoc-code-face) t) ((asciidoc-admonition-note-face) t) (nil nil)) ((nil nil) (asciidoc-code-face nil) (nil nil) (nil nil)) #("= Editing\n\nTEXT: inspect `code` first.\nContinue on this line.\n\nPlain paragraph.\n" 0 1 (face asciidoc-document-title-face) 2 10 (face asciidoc-document-title-face) 25 31 (face asciidoc-code-face)))"#
        ]],
    )
}

fn inline_parser_ranges_exclude_block_markers_and_macro_attribute_urls_stay_plain()
-> ParityBatchCase {
    ParityBatchCase::value(
        "inline_parser_ranges_exclude_block_markers_and_macro_attribute_urls_stay_plain",
        r##"(with-temp-buffer
  (insert
   "= Parser Ranges\n\n"
   "* bullet with `code`\n"
   "|===\n"
   "| cell with _text_\n"
   "|===\n\n"
   "image:badge.svg[Badge,link=\"https://example.com/x\"]\n"
   "A final paragraph with *bold*.\n")
  (asciidoc-mode)
  (font-lock-ensure)
  (let* ((block-root
          (treesit-buffer-root-node
           'asciidoc))
         (inline-root
          (treesit-buffer-root-node
           'asciidoc-inline))
         (inline-parser
          (car
           (treesit-parser-list
            nil 'asciidoc-inline)))
         (url-position
          (save-excursion
            (goto-char (point-min))
            (search-forward
             "https://example.com/x")
            (match-beginning 0))))
    (list
     (treesit-node-type block-root)
     (treesit-node-type inline-root)
     (string-match-p
      "ERROR"
      (treesit-node-string block-root))
     (string-match-p
      "ERROR"
      (treesit-node-string inline-root))
     (treesit-parser-included-ranges
      inline-parser)
     (get-text-property
      url-position 'face)
     (get-text-property
      url-position 'keymap)
     (get-text-property
      url-position 'mouse-face)
     (mapcar
      (lambda (property)
        (memq property
              font-lock-extra-managed-props))
      '(display keymap mouse-face
        follow-link help-echo)))))"##,
        expect![[
            r#"OK ("document" "inline" nil nil ((20 . 39) (45 . 63) (69 . 121) (121 . 152)) nil nil nil ((display . #1=(keymap . #2=(mouse-face . #3=(follow-link . #4=(help-echo))))) #1# #2# #3# #4#))"#
        ]],
    )
}

fn large_construct_dense_handbook_fontifies_stably_with_both_parsers_and_native_code()
-> ParityBatchCase {
    ParityBatchCase::value(
        "large_construct_dense_handbook_fontifies_stably_with_both_parsers_and_native_code",
        r##"(with-temp-buffer
  (insert
   "= Generated Operations Handbook\n"
   ":author: Reliability Team\n"
   ":toc: left\n\n")
  (dotimes (index 24)
    (insert
     (format
      (concat
       "== Service %02d\n\n"
       "[[service-%02d]]\n"
       "Service %02d uses *bold policy*, _careful prose_, and `runbook-%02d`.\n"
       "See https://example.com/services/%02d[service docs] and <<service-%02d>>.\n\n"
       "NOTE: Validate service %02d before deployment.\n"
       "The continuation records the rollback owner.\n\n"
       "* [x] configuration reviewed\n"
       "* [ ] rollback practiced\n\n"
       "[source,emacs-lisp]\n"
       "----\n"
       "(defun service-%02d-ready-p () t)\n"
       "----\n\n"
       "|===\n"
       "| Check | State\n"
       "| Configuration | Ready\n"
       "|===\n\n")
      index index index index index index
      index index)))
  (asciidoc-mode)
  (font-lock-ensure)
  (cl-labels
      ((snapshot
        ()
        (let ((position (point-min))
              runs)
          (while (< position (point-max))
            (let* ((face
                    (get-text-property
                     position 'face))
                   (next
                    (or
                     (next-single-property-change
                      position 'face nil
                      (point-max))
                     (point-max))))
              (when face
                (push
                 (list position next face)
                 runs))
              (setq position next)))
          (nreverse runs))))
    (let ((before (snapshot))
          (block-has-error
           (string-match-p
            "ERROR"
            (treesit-node-string
             (treesit-buffer-root-node
              'asciidoc))))
          (inline-has-error
           (string-match-p
            "ERROR"
            (treesit-node-string
             (treesit-buffer-root-node
              'asciidoc-inline)))))
      (font-lock-flush)
      (font-lock-ensure)
      (let ((after (snapshot)))
        (list
         (buffer-size)
         (count-lines
          (point-min) (point-max))
         (length before)
         (equal before after)
         block-has-error
         inline-has-error
         (seq-count
          (lambda (run)
            (eq (nth 2 run)
                'asciidoc-title-1-face))
          after)
         (seq-count
          (lambda (run)
            (eq (nth 2 run)
                'font-lock-keyword-face))
          after)
         (seq-count
          (lambda (run)
            (let ((face (nth 2 run)))
              (and
               (listp face)
               (memq
                'asciidoc-admonition-note-face
                face))))
          after)
         (secure-hash
          'sha256
          (prin1-to-string after)))))))"##,
        expect![[
            r#"OK (10318 532 510 t nil nil 24 24 48 "ba24d017bc1dee85e8946dd358bc978598b44a647739b133a3135dad5686a6d8")"#
        ]],
    )
}

pub(super) fn font_lock_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        construct_dense_document_applies_exact_semantic_faces_and_interaction_properties(),
        fontification_is_idempotent_across_flush_and_reparse_for_a_mixed_document(),
        source_language_extraction_and_mode_resolution_cover_options_aliases_and_fallbacks(),
        native_source_fontification_honors_enablement_size_language_and_recursion_guards(),
        editing_an_admonition_clears_multiline_background_and_preserves_inline_faces(),
        inline_parser_ranges_exclude_block_markers_and_macro_attribute_urls_stay_plain(),
        large_construct_dense_handbook_fontifies_stably_with_both_parsers_and_native_code(),
    ]
}
