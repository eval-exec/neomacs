use expect_test::expect;

use super::ParityBatchCase;

fn exact_descriptor_runtime_bytes_and_archive_file_set_identify_the_melpa_build() -> ParityBatchCase
{
    ParityBatchCase::value(
        "exact_descriptor_runtime_bytes_and_archive_file_set_identify_the_melpa_build",
        r##"(let* ((descriptor
         (cadr (assq 'asciidoc-mode package-alist)))
       (directory (package-desc-dir descriptor)))
  (list
   (featurep 'asciidoc-mode)
   (package-installed-p 'asciidoc-mode)
   (package-desc-name descriptor)
   (package-version-join (package-desc-version descriptor))
   (package-desc-reqs descriptor)
   (package-desc-summary descriptor)
   (package-desc-extras descriptor)
   (sort
    (directory-files directory nil "\\.el\\'")
    #'string<)
   (mapcar
    (lambda (name)
      (let ((path (expand-file-name name directory)))
        (list
         name
         (file-attribute-size (file-attributes path))
         (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-file-contents-literally path)
           (secure-hash 'sha256 (current-buffer))))))
    '("asciidoc-mode.el"
      "asciidoc-mode-pkg.el"))))"##,
        expect![[
            r#"OK (t t asciidoc-mode "20260612.645" ((emacs (30 1))) "Major mode for AsciiDoc markup." ((:maintainers ("Bozhidar Batsov" . "bozhidar@batsov.dev")) (:authors ("Bozhidar Batsov" . "bozhidar@batsov.dev")) (:keywords "text" "asciidoc" "languages" "tree-sitter") (:revdesc . "8914fad451f9") (:commit . "8914fad451f9c7f9c2286cf18db5edaa51a92cd7") (:url . "https://github.com/bbatsov/asciidoc-mode")) ("asciidoc-mode-autoloads.el" "asciidoc-mode-pkg.el" "asciidoc-mode.el") (("asciidoc-mode.el" 67472 "d5e134a7300204e8f0ff6c96b9087d20d960525119f5c7c98a764dc0c74d1735") ("asciidoc-mode-pkg.el" 464 "bcecf09c9f0bb1d746e3b90d96107ce8232694ab81e2113b35e9c72ec7e97cc3")))"#
        ]],
    )
}

fn complete_public_callable_surface_preserves_arguments_commands_and_source_ownership()
-> ParityBatchCase {
    ParityBatchCase::value(
        "complete_public_callable_surface_preserves_arguments_commands_and_source_ownership",
        r##"(mapcar
 (lambda (symbol)
   (list
    symbol
    (fboundp symbol)
    (macrop symbol)
    (commandp symbol)
    (copy-tree
     (help-function-arglist symbol t))
    (interactive-form symbol)
    (let ((file (symbol-file symbol 'defun)))
      (and file (file-name-nondirectory file)))))
 '(asciidoc-install-grammars
   asciidoc-promote-heading
   asciidoc-demote-heading
   asciidoc-follow-reference-at-point
   asciidoc-flymake
   asciidoc-mode))"##,
        expect![[
            r#"OK ((asciidoc-install-grammars t nil t nil (interactive nil) "asciidoc-mode.el") (asciidoc-promote-heading t nil t nil (interactive nil) "asciidoc-mode.el") (asciidoc-demote-heading t nil t nil (interactive nil) "asciidoc-mode.el") (asciidoc-follow-reference-at-point t nil t (&optional event) (interactive (list last-nonmenu-event)) "asciidoc-mode.el") (asciidoc-flymake t nil nil (report-fn &rest _args) nil "asciidoc-mode.el") (asciidoc-mode t nil t nil (interactive nil) "asciidoc-mode.el"))"#
        ]],
    )
}

fn generated_public_mode_variables_preserve_values_types_and_source_ownership() -> ParityBatchCase {
    ParityBatchCase::value(
        "generated_public_mode_variables_preserve_values_types_and_source_ownership",
        r##"(mapcar
 (lambda (symbol)
   (list
    symbol
    (boundp symbol)
    (cond
     ((eq symbol 'asciidoc-mode-hook)
      (default-value symbol))
     ((eq symbol 'asciidoc-mode-map)
      (keymapp (symbol-value symbol)))
     ((eq symbol 'asciidoc-mode-syntax-table)
      (let ((table (symbol-value symbol)))
        (list
         (syntax-table-p table)
         (aref table ?/)
         (aref table ?\n))))
     ((eq symbol 'asciidoc-mode-abbrev-table)
      (let ((table (symbol-value symbol)))
        (list
         (abbrev-table-p table)
         (abbrev-table-empty-p table)))))
    (let ((file (symbol-file symbol 'defvar)))
      (and file
           (file-name-nondirectory file)))))
 '(asciidoc-mode-hook
   asciidoc-mode-map
   asciidoc-mode-syntax-table
   asciidoc-mode-abbrev-table))"##,
        expect![[
            r#"OK ((asciidoc-mode-hook t nil "asciidoc-mode.el") (asciidoc-mode-map t t "asciidoc-mode.el") (asciidoc-mode-syntax-table t (t (3) (0)) "asciidoc-mode.el") (asciidoc-mode-abbrev-table t (t t) "asciidoc-mode.el"))"#
        ]],
    )
}

fn every_public_option_preserves_default_type_group_documentation_and_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_public_option_preserves_default_type_group_documentation_and_source",
        r##"(mapcar
 (lambda (symbol)
   (list
    symbol
    (default-value symbol)
    (eval (car (get symbol 'standard-value)))
    (get symbol 'custom-type)
    (get symbol 'custom-group)
    (documentation-property
     symbol 'variable-documentation t)
    (let ((file (symbol-file symbol 'defvar)))
      (and file (file-name-nondirectory file)))))
 '(asciidoc-fontify-code-blocks-natively
   asciidoc-code-lang-modes
   asciidoc-fontify-code-block-default-mode
   asciidoc-superscript-raise
   asciidoc-subscript-raise
   asciidoc-role-face-alist
   asciidoc-fontify-admonition-blocks
   asciidoc-asciidoctor-command
   asciidoc-asciidoctor-extra-args))"##,
        expect![[
            r#"OK ((asciidoc-fontify-code-blocks-natively 5000 5000 (choice (const :tag "Off" nil) (const :tag "All blocks" t) (integer :tag "Up to N characters")) nil "Whether to fontify source blocks using the language's major mode.\nWhen non-nil, the body of a `[source,LANG]' block is fontified with\nLANG's major mode (the same highlighting that mode would apply).  An\ninteger value only fontifies blocks whose body is at most that many\ncharacters, to avoid performance problems on very large blocks; a value\nof t fontifies all blocks regardless of size.  When nil, source block\nbodies keep the plain `font-lock-string-face' used for all verbatim\nblocks." "asciidoc-mode.el") (asciidoc-code-lang-modes #1=(("C" . c-mode) ("cpp" . c++-mode) ("C++" . c++-mode) ("bash" . sh-mode) ("shell" . sh-mode) ("elisp" . emacs-lisp-mode) ("json" js-json-mode json-ts-mode json-mode) ("ocaml" neocaml-mode tuareg-mode caml-mode) ("sqlite" . sql-mode)) #1# (alist :key-type string :value-type (choice (function :tag "Major mode") (repeat (function :tag "Major mode")))) nil "Alist mapping AsciiDoc source languages to major modes.\nUsed by native source block fontification when the major mode cannot be\nderived from the language name as LANG-mode.  The key is the language\nstring as it appears in the block (e.g. the `ruby' in `[source,ruby]').\nThe value is either a single major mode or a list of candidate modes\ntried in order, the first defined one being used -- so a language can map\nto a preferred mode with fallbacks.  For example `ocaml' maps to\n`neocaml-mode', then `tuareg-mode', then `caml-mode'." "asciidoc-mode.el") (asciidoc-fontify-code-block-default-mode prog-mode prog-mode function nil "Fallback major mode for native source block fontification.\nUsed when a block has no language, or no major mode can be found for its\nlanguage.  The default, `prog-mode', applies no highlighting." "asciidoc-mode.el") (asciidoc-superscript-raise 0.4 0.4 number nil "How far to raise superscript text, as a fraction of line height.\nApplied as a `display' \\='(raise ...) property on top of\n`asciidoc-superscript-face'." "asciidoc-mode.el") (asciidoc-subscript-raise -0.25 -0.25 number nil "How far to lower subscript text, as a fraction of line height.\nApplied as a `display' \\='(raise ...) property on top of\n`asciidoc-subscript-face'." "asciidoc-mode.el") (asciidoc-role-face-alist #2=(("line-through" . asciidoc-strike-through-face) ("underline" . asciidoc-underline-face) ("overline" . asciidoc-overline-face)) #2# (alist :key-type (string :tag "Role name") :value-type (face :tag "Face")) nil "Alist mapping AsciiDoc role names to faces for custom-style spans.\nRecognised inside `[.role]#text#' (and the unconstrained `##' variant).\nA span whose role is not listed keeps the default face." "asciidoc-mode.el") (asciidoc-fontify-admonition-blocks t t boolean nil "When non-nil, tint the whole body of a paragraph admonition.\nThe label is always color-coded; this controls only the background that\nspans the admonition's body lines." "asciidoc-mode.el") (asciidoc-asciidoctor-command "asciidoctor" "asciidoctor" string nil "Executable used by the Asciidoctor-backed Flymake checker." "asciidoc-mode.el") (asciidoc-asciidoctor-extra-args nil nil (repeat string) nil "Extra command-line arguments passed to Asciidoctor by the Flymake checker." "asciidoc-mode.el"))"#
        ]],
    )
}

fn public_constants_keymaps_menu_and_customization_group_are_complete() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_constants_keymaps_menu_and_customization_group_are_complete",
        r##"(list
 asciidoc-mode-version
 asciidoc-grammar-recipes
 (mapcar
  (lambda (key)
    (list
     key
     (keymap-lookup
      asciidoc-reference-map key)))
  '("RET" "<mouse-2>"))
 (mapcar
  (lambda (key)
    (list
     key
     (keymap-lookup asciidoc-mode-map key)))
  '("M-<left>" "M-<right>"
    "M-<up>" "M-<down>"
    "C-c C-n" "C-c C-p"
    "C-c C-f" "C-c C-b"
    "C-c C-u" "C-c C-o"))
 (keymapp asciidoc-mode-menu)
 (car asciidoc-mode-menu)
 (get 'asciidoc 'group-documentation)
 (sort
  (mapcar #'car
          (copy-tree
           (get 'asciidoc 'custom-group)))
  (lambda (left right)
    (string<
     (symbol-name left)
     (symbol-name right))))
 (get 'asciidoc-mode 'flyspell-mode-predicate))"##,
        expect![[
            r#"OK ("0.4.0" ((asciidoc "https://github.com/cathaysia/tree-sitter-asciidoc" nil "tree-sitter-asciidoc/src") (asciidoc-inline "https://github.com/cathaysia/tree-sitter-asciidoc" nil "tree-sitter-asciidoc_inline/src")) (("RET" asciidoc-follow-reference-at-point) ("<mouse-2>" asciidoc-follow-reference-at-point)) (("M-<left>" asciidoc-promote-heading) ("M-<right>" asciidoc-demote-heading) ("M-<up>" outline-move-subtree-up) ("M-<down>" outline-move-subtree-down) ("C-c C-n" outline-next-visible-heading) ("C-c C-p" outline-previous-visible-heading) ("C-c C-f" outline-forward-same-level) ("C-c C-b" outline-backward-same-level) ("C-c C-u" outline-up-heading) ("C-c C-o" asciidoc-follow-reference-at-point)) t keymap "Support for AsciiDoc markup." (asciidoc-admonition-caution-face asciidoc-admonition-caution-label-face asciidoc-admonition-important-face asciidoc-admonition-important-label-face asciidoc-admonition-note-face asciidoc-admonition-note-label-face asciidoc-admonition-tip-face asciidoc-admonition-tip-label-face asciidoc-admonition-warning-face asciidoc-admonition-warning-label-face asciidoc-anchor-face asciidoc-asciidoctor-command asciidoc-asciidoctor-extra-args asciidoc-code-face asciidoc-code-lang-modes asciidoc-cross-reference-face asciidoc-document-title-face asciidoc-fontify-admonition-blocks asciidoc-fontify-code-block-default-mode asciidoc-fontify-code-blocks-natively asciidoc-footnote-marker-face asciidoc-footnote-text-face asciidoc-highlight-face asciidoc-link-face asciidoc-link-mouse-face asciidoc-markup-face asciidoc-metadata-key-face asciidoc-metadata-value-face asciidoc-overline-face asciidoc-role-face-alist asciidoc-strike-through-face asciidoc-subscript-face asciidoc-subscript-raise asciidoc-superscript-face asciidoc-superscript-raise asciidoc-title-1-face asciidoc-title-2-face asciidoc-title-3-face asciidoc-title-4-face asciidoc-title-5-face asciidoc-underline-face asciidoc-url-face) asciidoc--flyspell-verify)"#
        ]],
    )
}

fn every_package_face_exists_with_exact_group_inheritance_and_source_ownership() -> ParityBatchCase
{
    ParityBatchCase::value(
        "every_package_face_exists_with_exact_group_inheritance_and_source_ownership",
        r##"(mapcar
 (lambda (face)
   (list
    face
    (facep face)
    (face-attribute face :inherit nil t)
    (face-attribute face :weight nil t)
    (face-attribute face :underline nil t)
    (face-attribute face :overline nil t)
    (face-attribute face :strike-through nil t)
    (get face 'custom-group)
    (let ((file (symbol-file face 'defface)))
      (and file (file-name-nondirectory file)))))
 (sort
  (seq-filter
   #'facep
   (apropos-internal
    "^asciidoc-.*-face$"))
  (lambda (left right)
    (string<
     (symbol-name left)
     (symbol-name right)))))"##,
        expect![[
            r#"OK ((asciidoc-admonition-caution-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-caution-label-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] warning bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-important-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-important-label-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] font-lock-warning-face bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-note-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-note-label-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] font-lock-keyword-face bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-tip-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-tip-label-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] success bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-warning-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-admonition-warning-label-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] error bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-anchor-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] shadow unspecified unspecified t unspecified nil "asciidoc-mode.el") (asciidoc-code-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] (font-lock-string-face fixed-pitch) unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-cross-reference-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] link unspecified t unspecified unspecified nil "asciidoc-mode.el") (asciidoc-document-title-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] outline-1 bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-footnote-marker-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] font-lock-function-call-face bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-footnote-text-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] font-lock-comment-face bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-highlight-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] highlight unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-link-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] link unspecified t unspecified unspecified nil "asciidoc-mode.el") (asciidoc-link-mouse-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] highlight unspecified t unspecified unspecified nil "asciidoc-mode.el") (asciidoc-markup-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] shadow unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-metadata-key-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] font-lock-variable-name-face bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-metadata-value-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] font-lock-string-face unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-overline-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified t unspecified nil "asciidoc-mode.el") (asciidoc-strike-through-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified unspecified t nil "asciidoc-mode.el") (asciidoc-subscript-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-superscript-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-title-1-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] outline-2 bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-title-2-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] outline-3 bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-title-3-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] outline-4 bold unspecified unspecified unspecified nil "asciidoc-mode.el") (asciidoc-title-4-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] outline-5 bold t unspecified unspecified nil "asciidoc-mode.el") (asciidoc-title-5-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] outline-6 bold t unspecified unspecified nil "asciidoc-mode.el") (asciidoc-underline-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified t unspecified unspecified nil "asciidoc-mode.el") (asciidoc-url-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] font-lock-string-face unspecified unspecified unspecified unspecified nil "asciidoc-mode.el"))"#
        ]],
    )
}

fn generated_autoloads_register_only_the_two_commands_and_both_file_extensions() -> ParityBatchCase
{
    ParityBatchCase::value(
        "generated_autoloads_register_only_the_two_commands_and_both_file_extensions",
        r##"(list
 (featurep 'asciidoc-mode)
 (featurep 'asciidoc-mode-autoloads)
 (mapcar
  (lambda (symbol)
    (list
     symbol
     (fboundp symbol)
     (and
      (fboundp symbol)
      (autoloadp (symbol-function symbol)))
     (commandp symbol)
     (copy-tree
      (help-function-arglist symbol t))))
  '(asciidoc-install-grammars
    asciidoc-mode
    asciidoc-promote-heading
    asciidoc-flymake))
 (mapcar
  (lambda (regexp)
    (cons regexp
          (cdr (assoc regexp auto-mode-alist))))
  '("\\.adoc\\'"
    "\\.asciidoc\\'"))
 (boundp 'asciidoc-fontify-code-blocks-natively)
 (boundp 'asciidoc-mode-map))"##,
        expect![[
            r#"OK (nil t ((asciidoc-install-grammars t t t "[Arg list not available until function definition is loaded.]") (asciidoc-mode t t t "[Arg list not available until function definition is loaded.]") (asciidoc-promote-heading nil nil nil t) (asciidoc-flymake nil nil nil t)) (("\\.adoc\\'" . asciidoc-mode) ("\\.asciidoc\\'" . asciidoc-mode)) nil nil)"#
        ]],
    )
}

pub(super) fn surface_asciidoc_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        exact_descriptor_runtime_bytes_and_archive_file_set_identify_the_melpa_build(),
        complete_public_callable_surface_preserves_arguments_commands_and_source_ownership(),
        generated_public_mode_variables_preserve_values_types_and_source_ownership(),
        every_public_option_preserves_default_type_group_documentation_and_source(),
        public_constants_keymaps_menu_and_customization_group_are_complete(),
        every_package_face_exists_with_exact_group_inheritance_and_source_ownership(),
    ]
}

pub(super) fn surface_asciidoc_mode_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![generated_autoloads_register_only_the_two_commands_and_both_file_extensions()]
}
