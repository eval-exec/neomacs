use expect_test::expect;

use super::ParityBatchCase;

/// The mode's front door: `auto-mode-alist' claims `.adoc' and `.asciidoc' but
/// not `.txt', and visiting such a file gives the AsciiDoc editing environment
/// -- comment syntax, the outline configuration the heading commands navigate
/// with, the font-lock configuration (multiline keywords, extra managed
/// properties, its own unfontify function), the completion, xref and fill hooks
/// the mode installs buffer-locally, and its key bindings.
fn visiting_an_adoc_file_sets_up_the_asciidoc_editing_environment() -> ParityBatchCase {
    ParityBatchCase::value(
        "visiting_an_adoc_file_sets_up_the_asciidoc_editing_environment",
        r##"(let ((buffer (adoc-test-open "docs/guide.adoc" adoc-test-guide)))
  (unwind-protect
      (with-current-buffer buffer
        (list
         :mode (list major-mode mode-name (and (derived-mode-p 'text-mode) t))
         :comments (list comment-start comment-end comment-start-skip comment-column)
         :outline (list outline-regexp (and (bound-and-true-p outline-minor-mode) t))
         :font-lock (list (car font-lock-defaults)
                          font-lock-extra-managed-props
                          font-lock-unfontify-region-function
                          (and (memq #'adoc-font-lock-extend-region
                                     font-lock-extend-region-functions)
                               t))
         :hooks (list (and (memq #'adoc-completion-at-point
                                 completion-at-point-functions)
                           t)
                      (and (memq #'adoc--xref-backend xref-backend-functions) t)
                      (and (memq #'adoc-fill-nobreak-p fill-nobreak-predicate) t)
                      fill-paragraph-function
                      imenu-create-index-function
                      page-delimiter
                      parse-sexp-lookup-properties)
         :keys (mapcar (lambda (key) (cons key (key-binding (kbd key))))
                       '("C-c C-n" "C-c C-p" "C-c C-u" "M-<left>" "M-<right>"
                         "M-RET" "TAB" "C-c C-t" "C-c C-s b" "M-."))
         :auto-mode (mapcar (lambda (name)
                              (cons name (assoc-default name auto-mode-alist
                                                        #'string-match-p)))
                            '("guide.adoc" "guide.asciidoc" "guide.txt"
                              "guide.adoc~"))
         :buffer (list (buffer-modified-p) (point) (buffer-size))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (:mode (adoc-mode "adoc" t) :comments ("// " "" "^//[ \11]*" 0) :outline ("=\\{1,6\\}[ \11]+[^ \11\n]" t) :font-lock (adoc-font-lock-keywords (adoc-reserved adoc-attribute-list adoc-code-block adoc-flyspell-ignore) adoc-unfontify-region-function t) :hooks (t t t adoc-fill-paragraph adoc-imenu-create-nested-index "^<<<+$" t) :keys (("C-c C-n" . adoc-next-visible-heading) ("C-c C-p" . adoc-previous-visible-heading) ("C-c C-u" . adoc-up-heading) ("M-<left>" . adoc-promote) ("M-<right>" . adoc-demote) ("M-RET" . adoc-insert-list-item) ("TAB" . adoc-cycle) ("C-c C-t" . adoc-toggle-title-type) ("C-c C-s b" . adoc-insert-bold) ("M-." . adoc-follow-thing-at-point)) :auto-mode (("guide.adoc" . adoc-mode) ("guide.asciidoc" . adoc-mode) ("guide.txt" . text-mode) ("guide.adoc~")) :buffer (nil 1 527))"#
        ]],
    )
}

fn font_lock_marks_up_every_construct_of_a_realistic_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_marks_up_every_construct_of_a_realistic_document",
        r##"(let ((buffer (adoc-test-open "docs/guide.adoc" adoc-test-guide)))
  (unwind-protect
      (with-current-buffer buffer
        (list
         :header (adoc-test-faces '(("= Field Guide" 0) ("Field Guide" 0)
                                    (":toc:" 0) ("left" 0)
                                    (":sourcedir:" 0) ("./src" 0)))
         :titles (adoc-test-faces '(("== Getting Started" 0) ("Getting Started" 0)
                                    ("=== Configuration" 0) ("Configuration" 0)
                                    ("== Troubleshooting" 0) ("Troubleshooting" 0)))
         :inline (adoc-test-faces '(("*bold*" 0) ("*bold*" 1)
                                    ("_italic_" 1) ("`monospace`" 1)
                                    ("https://example.org/widgets" 0)
                                    ("widget catalogue" 0)
                                    ("{sourcedir}" 0)))
         :blocks (adoc-test-faces '((". Download" 0) ("* First bullet" 0)
                                    ("NOTE:" 0) ("WARNING:" 0)
                                    ("[source,ruby]" 0) ("----" 0)
                                    ("def widget" 0) ("puts" 0)))
         :source-block (save-excursion
                         (goto-char (point-min))
                         (search-forward "def widget")
                         (adoc-test-face-runs (line-beginning-position)
                                              (line-end-position 3)))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (:header (("= Field Guide" . adoc-meta-hide-face) ("Field Guide" . adoc-title-0-face) (":toc:" . adoc-metadata-key-face) ("left" . adoc-metadata-value-face) (":sourcedir:" . adoc-metadata-key-face) ("./src" . adoc-metadata-value-face)) :titles (("== Getting Started" . adoc-meta-hide-face) ("Getting Started" . adoc-title-1-face) ("=== Configuration" . adoc-meta-hide-face) ("Configuration" . adoc-title-2-face) ("== Troubleshooting" . adoc-meta-hide-face) ("Troubleshooting" . adoc-title-1-face)) :inline (("*bold*" . adoc-meta-hide-face) ("*bold*" adoc-bold-face) ("_italic_" adoc-emphasis-face) ("`monospace`" adoc-typewriter-face adoc-verbatim-face) ("https://example.org/widgets" . adoc-url-face) ("widget catalogue" . adoc-reference-face) ("{sourcedir}" . adoc-replacement-face)) :blocks ((". Download" . adoc-list-face) ("* First bullet" . adoc-list-face) ("NOTE:" . adoc-complex-replacement-face) ("WARNING:" . adoc-complex-replacement-face) ("[source,ruby]" . adoc-meta-face) ("----" . adoc-meta-face) ("def widget" font-lock-keyword-face . #1=(adoc-native-code-face)) ("puts" font-lock-builtin-face . #1#)) :source-block (("def" font-lock-keyword-face . #2=(adoc-native-code-face)) (" " . #2#) ("widget" font-lock-function-name-face . #2#) ("(name)\n  " . #2#) ("puts" font-lock-builtin-face . #2#) (" " . #2#) ("\"building\"" font-lock-string-face . #2#) ("\n" . #2#) ("end" font-lock-keyword-face . #2#)))"#
        ]],
    )
}

fn imenu_indexes_the_documents_headings_nested_and_flat() -> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_indexes_the_documents_headings_nested_and_flat",
        r##"(let ((buffer (adoc-test-open "docs/guide.adoc" adoc-test-guide)))
  (unwind-protect
      (with-current-buffer buffer
        (font-lock-ensure)
        (list
         :default-function imenu-create-index-function
         :nested (adoc-test-plain (funcall imenu-create-index-function))
         :flat (adoc-test-plain (adoc-imenu-create-index))
         :titles-at (adoc-test-plain
                     (mapcar (lambda (position)
                               (save-excursion
                                 (goto-char position)
                                 (adoc-test-line)))
                             (mapcar #'cdr (adoc-imenu-create-index))))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (:default-function adoc-imenu-create-nested-index :nested (("Field Guide to Widgets" (nil . 1) ("Getting Started" (nil . 201) ("Configuration" . 400)) ("Troubleshooting" . 467))) :flat (("Field Guide to Widgets" . 1) ("Getting Started" . 201) ("Configuration" . 400) ("Troubleshooting" . 467)) :titles-at ("= Field Guide to Widgets" "== Getting Started" "=== Configuration" "== Troubleshooting"))"#
        ]],
    )
}

fn promote_demote_and_toggle_restructure_a_section_title() -> ParityBatchCase {
    ParityBatchCase::value(
        "promote_demote_and_toggle_restructure_a_section_title",
        r##"(let ((buffer (adoc-test-open "docs/guide.adoc" adoc-test-guide)))
  (unwind-protect
      (with-current-buffer buffer
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "=== Configuration")
        (beginning-of-line)
        (let ((before (adoc-test-where)))
          (execute-kbd-macro (kbd "M-<left>"))
          (let ((promoted (adoc-test-where)))
            (execute-kbd-macro (kbd "M-<right>"))
            (execute-kbd-macro (kbd "M-<right>"))
            (let ((demoted (adoc-test-where)))
              (execute-kbd-macro (kbd "C-c C-t"))
              (list :before before
                    :promoted promoted
                    :demoted demoted
                    :toggled (adoc-test-lines 2)
                    :imenu (adoc-test-plain (adoc-imenu-create-index))
                    :modified (buffer-modified-p))))))
    (progn (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:before (:point 400 :column 0 :line-number 26 :line "=== Configuration") :promoted (:point 400 :column 0 :line-number 26 :line "==== Configuration ====") :demoted (:point 400 :column 0 :line-number 26 :line "== Configuration ==") :toggled "Configuration\n-------------" :imenu (("Field Guide to Widgets" . 1) ("Getting Started" . 201) ("Troubleshooting" . 477)) :modified t)"#
        ]],
    )
}

fn the_styling_keys_wrap_and_unwrap_asciidoc_emphasis() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_styling_keys_wrap_and_unwrap_asciidoc_emphasis",
        r##"(let ((buffer (adoc-test-open "docs/style.adoc"
                              "= Styling\n\nA short bold intro with plain text.\n")))
  (unwind-protect
      (with-current-buffer buffer
        (transient-mark-mode 1)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "bold")
        (execute-kbd-macro (kbd "C-c C-s b"))
        (let ((bolded (adoc-test-where)))
          (goto-char (point-min))
          (search-forward "*bold*")
          (goto-char (1+ (match-beginning 0)))
          (execute-kbd-macro (kbd "C-SPC C-f C-f C-f C-f"))
          (let ((selection (list (region-active-p)
                                 (buffer-substring-no-properties (region-beginning)
                                                                 (region-end)))))
            (execute-kbd-macro (kbd "C-c C-s b"))
            (let ((unbolded (adoc-test-where)))
              (goto-char (point-min))
              (search-forward "plain")
              (goto-char (match-beginning 0))
              (execute-kbd-macro (kbd "C-SPC M-f M-f"))
              (execute-kbd-macro (kbd "C-c C-s i"))
              (let ((italic (adoc-test-where)))
                (goto-char (point-min))
                (search-forward "short")
                (execute-kbd-macro (kbd "C-c C-s m"))
                (list :bolded bolded
                      :selection selection
                      :unbolded unbolded
                      :italic italic
                      :monospace (adoc-test-line)
                      :faces (adoc-test-faces '(("`short`" 1) ("_plain text_" 1)))
                      :text (buffer-substring-no-properties (point-min)
                                                            (point-max))))))))
    (progn (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:bolded (:point 26 :column 14 :line-number 3 :line "A short *bold* intro with plain text.") :selection (t "bold") :unbolded (:point 24 :column 12 :line-number 3 :line "A short bold intro with plain text.") :italic (:point 48 :column 36 :line-number 3 :line "A short bold intro with _plain text_.") :monospace "A `short` bold intro with _plain text_." :faces (("`short`" adoc-typewriter-face adoc-verbatim-face) ("_plain text_" adoc-emphasis-face)) :text "= Styling\n\nA `short` bold intro with _plain text_.\n")"#
        ]],
    )
}

fn the_mode_handles_a_document_written_in_non_ascii_prose() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_mode_handles_a_document_written_in_non_ascii_prose",
        r##"(let ((buffer (adoc-test-open "docs/unicode.adoc" adoc-test-unicode)))
  (unwind-protect
      (with-current-buffer buffer
        (font-lock-ensure)
        (let ((faces (adoc-test-faces '(("日本語ハンドブック" 0)
                                        ("Café Notes" 0)
                                        ("Ünicode Anhang" 0)
                                        ("*太字*" 1) ("_斜体_" 1) ("`等幅`" 1)
                                        ("TIP:" 0)
                                        ("* 項目 1" 0) ("* Élément 2" 0)
                                        (":author:" 0) ("Renée" 0))))
              (index (adoc-test-plain (adoc-imenu-create-index))))
          (goto-char (point-min))
          (search-forward "Une phrase")
          (let ((fill-column 40))
            (fill-paragraph))
          (list :faces faces
                :index index
                :size (list (buffer-size) (point-max))
                :filled (adoc-test-lines 4))))
    (progn (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:faces (("日本語ハンドブック" . adoc-title-0-face) ("Café Notes" . adoc-title-1-face) ("Ünicode Anhang" . adoc-title-2-face) ("*太字*" adoc-bold-face) ("_斜体_" adoc-emphasis-face) ("`等幅`" adoc-typewriter-face adoc-verbatim-face) ("TIP:" . adoc-complex-replacement-face) ("* 項目 1" . adoc-list-face) ("* Élément 2" . adoc-list-face) (":author:" . adoc-metadata-key-face) ("Renée" . adoc-metadata-value-face)) :index (("日本語ハンドブック" . 1) ("Café Notes — Grüße" . 35) ("Ünicode Anhang" . 181)) :size (336 337) :filled "Une phrase française assez longue pour\nêtre remplie sur plusieurs lignes par la\ncommande de remplissage standard d'Emacs\nsans problème.")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        visiting_an_adoc_file_sets_up_the_asciidoc_editing_environment(),
        font_lock_marks_up_every_construct_of_a_realistic_document(),
        imenu_indexes_the_documents_headings_nested_and_flat(),
        promote_demote_and_toggle_restructure_a_section_title(),
        the_styling_keys_wrap_and_unwrap_asciidoc_emphasis(),
        the_mode_handles_a_document_written_in_non_ascii_prose(),
    ]
}
