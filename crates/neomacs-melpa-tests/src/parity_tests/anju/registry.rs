use expect_test::expect;

use super::ParityBatchCase;

fn anju_exact_pin_dependency_graph_and_features_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_exact_pin_dependency_graph_and_features_match",
        r##"(let ((descriptor
                    (cadr (assq 'anju package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join (package-desc-version descriptor))
          (package-desc-reqs descriptor)
          (package-desc-summary descriptor)
          (copy-tree (package-desc-extras descriptor))
          (package-desc-kind descriptor)
          (package-desc-archive descriptor)
          (mapcar
           (lambda (feature)
             (list feature (featurep feature)))
           '(anju
             anju-utils
             anju-style-text
             anju-mode-line
             anju-main-menu
             anju-context-menu
             markdown-mode
             casual-dired
             casual-org
             casual-compile
             casual-make
             casual-agenda))))"##,
        expect![[
            r#"OK (anju "20260701.2139" ((emacs (29 1)) (casual (2 14 0)) (markdown-mode (2 7))) "Mouse UX Customizations." ((:maintainers ("Charles Choi" . "charles.choi@yummymelon.com")) (:authors ("Charles Choi" . "charles.choi@yummymelon.com")) (:keywords "tools") (:revdesc . "f5d27108ffe5") (:commit . "f5d27108ffe5facb6886fab191068efd1faea39f") (:url . "https://github.com/kickingvegas/anju")) nil nil ((anju t) (anju-utils t) (anju-style-text t) (anju-mode-line t) (anju-main-menu t) (anju-context-menu t) (markdown-mode t) (casual-dired t) (casual-org t) (casual-compile t) (casual-make t) (casual-agenda t)))"#
        ]],
    )
}

fn anju_public_and_internal_command_surface_is_callable() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_public_and_internal_command_surface_is_callable",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (fboundp symbol)
            (commandp symbol)
            (documentation symbol)))
         '(anju-init
           anju-style-bold
           anju-style-italic
           anju-style-code
           anju-style-underline
           anju-style-verbatim
           anju-style-strike-through
           anju-style-remove
           anju-style-dwim
           anju-toggle-one-window
           anju-dired-duplicate-file
           anju-org-table-recalculate
           anju-copy-raw-link
           anju-info-goto-node-web
           anju-ert-run-test-at-point
           anju-edebug-defun
           anju-extract-lambda-to-defun
           anju-occur-selected-region
           anju-yank-markdown-as-org
           anju-org-copy-region-as-markdown
           anju-org-copy-region-as-gfm
           anju-org-copy-region-as-latex
           anju-org-copy-region-as-ascii
           anju-org-copy-region-as-html
           anju-org-copy-region-as-rtf
           anju-popup-window-management-menu
           anju-popup-buffer-menu
           anju-reconfigure-context-menu-functions
           anju-reset-context-menu-functions))"##,
        expect![[
            r#"OK ((anju-init t t "Reconfigure Emacs mouse menus and bindings to Anju specification.\n\nThis initialization command for Anju reconfigures the following areas\nof mouse menus and bindings:\n\n- Legacy mouse bindings (‘anju-unset-legacy-mouse-bindings-enable’)\n- Mode line bindings (‘anju-mode-line-bindings-enable’)\n- Main menu (‘anju-reconfigure-main-menu-enable’)\n- Context menus (‘anju-reconfigure-context-menu-functions-enable’)\n\nEach area is controlled with a customizable variable and all are by\ndefault t. Changes to any of these variables will require a restart of\nEmacs.\n\nThe global minor mode ‘context-menu-mode’ will be initialized if it\nalready has not been done so.") (anju-style-bold t t "Mark region bold for modes which are supported by ‘anju-style-text’.") (anju-style-italic t t "Mark region italic for modes which are supported by ‘anju-style-text’.") (anju-style-code t t "Mark region code for modes which are supported by ‘anju-style-text’.") (anju-style-underline t t "Mark region underline for Org mode.") (anju-style-verbatim t t "Mark region verbatim for Org mode.") (anju-style-strike-through t t "Mark region strike-through for modes which are supported by ‘anju-style-text’.") (anju-style-remove t t "Remove marked region.") (anju-style-dwim t t "DWIM emphasize text for modes supported by ‘anju-style-text’.\n\nThis command will appropriately style either a region or the text\nthe point is in depending on whether the current major mode is\nOrg or Markdown. Selection of the emphasis style is done by\nmini-buffer command completion.\n\nIf no region is defined, then the text amount is considered to be\na balanced expression (sexp). A balanced expression is used as it\ncan cover most cases of applying the style to text that is\ncontiguous without spaces.") (anju-toggle-one-window t t "Make WINDOW fill its frame.\n- INTERACTIVE is passed to ‘delete-other-windows’.") (anju-dired-duplicate-file t t "Duplicate the current file in Dired.") (anju-org-table-recalculate t t "Recalculate an Org table.") (anju-copy-raw-link t t "Copy raw link from an Org hyperlink.") (anju-info-goto-node-web t t "Open node in web browser.") (anju-ert-run-test-at-point t t "Run the ERT test at point.") (anju-edebug-defun t t "Convenience function to instrument function for Edebug.") (anju-extract-lambda-to-defun t t "Extract lambda expression at point to defun named ARG.\n\nWhen the point is on a lambda symbol, this command will prompt for a\nfunction name ARG and will convert the lambda expression into a defun.\nThe new defun is not evaluated.\n\nThis converted function is put into a temporary buffer ‘*ARG*’ for\nsubsequent editing while the original lambda expression is replaced with\na reference to the new defun ARG.") (anju-occur-selected-region t t "Occur selected region.") (anju-yank-markdown-as-org t t "Yank Markdown text as Org.\n\nThis command will convert Markdown text in the top of the ‘kill-ring’\nand convert it to Org using the pandoc utility.") (anju-org-copy-region-as-markdown t t "Copy the Markdown exported Org region to the system clipboard.") (anju-org-copy-region-as-gfm t t "Copy the GitHub Markdown exported Org region to the system clipboard.") (anju-org-copy-region-as-latex t t "Copy the LaTeX exported Org region to the system clipboard.") (anju-org-copy-region-as-ascii t t "Copy the ASCII exported Org region to the system clipboard.") (anju-org-copy-region-as-html t t "Copy the HTML exported Org region to the system clipboard.") (anju-org-copy-region-as-rtf t t "Export region to RTF and copy it to the clipboard.\n\nCode from Daniel Martin\nURL ‘https://gist.github.com/danielmartin/3c5d3a3a8cd24a3556379c5251651748’.") (anju-popup-window-management-menu t t "Popup mouse window management with CLICK.") (anju-popup-buffer-menu t t "Popup mouse buffer navigation with CLICK.") (anju-reconfigure-context-menu-functions t t "Reconfigure ‘context-menu-functions’.") (anju-reset-context-menu-functions t t "Reset ‘context-menu-functions’."))"#
        ]],
    )
}

fn anju_installed_payload_inventory_is_exact_and_unvendored() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_installed_payload_inventory_is_exact_and_unvendored",
        r##"(let* ((descriptor
                  (cadr (assq 'anju package-alist)))
                 (directory (package-desc-dir descriptor)))
         (mapcar
          (lambda (name)
            (let ((path (expand-file-name name directory)))
              (cond
               ((file-directory-p path)
                (list name :directory
                      (sort (directory-files path nil "\\`[^.]") #'string<)))
               ((string-suffix-p ".elc" name)
                (list name :compiled (file-regular-p path)
                      (> (nth 7 (file-attributes path)) 0)))
               (t
                (with-temp-buffer
                  (set-buffer-multibyte nil)
                  (insert-file-contents-literally path)
                  (list name (buffer-size)
                        (secure-hash 'sha256 (current-buffer))))))))
          (sort (directory-files directory nil "\\`[^.]") #'string<)))"##,
        expect![[
            r#"OK (("anju-autoloads.el" 1202 "15ea81d486ab5a447e561f0b64d0cc3d256f3223bbfe4ad7bb6d1930e5e701c0") ("anju-context-menu.el" 71180 "13e105419b71d46964519c07a6ce83d4954962de48a9fd6299ed2425b977cc25") ("anju-context-menu.elc" :compiled t t) ("anju-main-menu.el" 22893 "c687e3f6b5c6f488a6af2bc94e2796a61677ec537bfcfe817312c9bb424ceefe") ("anju-main-menu.elc" :compiled t t) ("anju-mode-line.el" 10872 "f63d3372f95ee6b22b91c71f40b2f97304f3176d216b1d6f6ca573d2d1b64cb9") ("anju-mode-line.elc" :compiled t t) ("anju-pkg.el" 482 "18db0d9f08fb0fa77aed3380b88c70cf2d0388545743a3c5861487fb57863d19") ("anju-style-text.el" 5452 "57802cd520d90f0761113d2d3b63e85e19e4b5d2e5313f385717098d63520fa2") ("anju-style-text.elc" :compiled t t) ("anju-utils.el" 16419 "8a038505dff1f7d2f4fb25e4be95f81bf5e41a47e3cf3c0b7f8ae968fa7ab815") ("anju-utils.elc" :compiled t t) ("anju.el" 3915 "163e0638e5f2dadc64b9aaf95bf96bcf9d20b42e87b4ba267aa8d716ddf04446") ("anju.elc" :compiled t t) ("anju.info" 84986 "f6ae9117852864e2b77d2a649860ae93fbe3af87c11756eb72e00489256c3920") ("dir" 681 "51e6dfdf3c672cf14a2d8bad09556ec42737964fc8d70a65abb3f94c91da2360") ("images" :directory ("anju-context-menu-compilation.png" "anju-context-menu-customize.png" "anju-context-menu-dired-copy-to.png" "anju-context-menu-dired-insert-subdir.png" "anju-context-menu-edebug.png" "anju-context-menu-elisp-built-in.png" "anju-context-menu-elisp.png" "anju-context-menu-enhanced-paste.png" "anju-context-menu-ert-result.png" "anju-context-menu-ert.png" "anju-context-menu-extract-lambda-1.png" "anju-context-menu-extract-lambda-2.png" "anju-context-menu-extract-lambda-3.png" "anju-context-menu-info.png" "anju-context-menu-makefile.png" "anju-context-menu-org-agenda.png" "anju-context-menu-org-copy-as.png" "anju-context-menu-org-headline.png" "anju-context-menu-org-item-inprogress.png" "anju-context-menu-org-item.png" "anju-context-menu-org-link-copy-address.png" "anju-context-menu-org-link.png" "anju-context-menu-org-table.png" "anju-context-menu-rectangle.png" "anju-context-menu-region.png" "anju-context-menu-toggle-images-markup.png" "anju-context-menu-vc.png" "anju-context-menu-xref.png" "anju-kmacro-menu-recording.png" "anju-kmacro-menu.png" "anju-main-menu-bookmarks.png" "anju-main-menu-edit-search-in-files.png" "anju-main-menu-help.png" "anju-main-menu-imenu.png" "anju-main-menu-registers.png" "anju-main-menu-text.png" "anju-main-menu-tools-kmacro.png" "anju-main-menu-tools.png" "anju-mode-line-buffer-list.png" "anju-mode-line-window-management.png" "default-yellow.png")))"#
        ]],
    )
}

fn anju_source_and_customization_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_source_and_customization_inventory_matches",
        r##"(list
         (mapcar
         (lambda (symbol)
            (let ((standard
                   (get symbol 'standard-value)))
              (list
               symbol
               (default-value symbol)
               (and standard
                    (eval (car standard)))
               (get symbol 'custom-type)
               (get symbol 'custom-group))))
          '(anju-reconfigure-context-menu-functions-enable
            anju-reconfigure-main-menu-enable
            anju-unset-legacy-mouse-bindings-enable
            anju-mode-line-bindings-enable
            anju-help-menu-remove-emacs-tutorial
            anju-file-menu-replace-make-frame-on
            anju-buffer-list-filter-functions
            anju-reconfigure-main-menu-hook
            anju-mode-line-buffer-list-function))
         (copy-tree anju-context-menu--inventory))"##,
        expect![[
            r#"OK (((anju-reconfigure-context-menu-functions-enable t t boolean nil) (anju-reconfigure-main-menu-enable t t boolean nil) (anju-unset-legacy-mouse-bindings-enable t t boolean nil) (anju-mode-line-bindings-enable t t boolean nil) (anju-help-menu-remove-emacs-tutorial nil nil boolean nil) (anju-file-menu-replace-make-frame-on t t boolean nil) (anju-buffer-list-filter-functions #1=((anju-buffer-list-project-filter . 7) (anju-buffer-list-compilation-filter . 3) (anju-buffer-list-grep-filter . 3) (anju-buffer-list-xref-filter . 3) (anju-buffer-list-eshell-filter . 3) (anju-buffer-list-shell-filter . 3) (anju-buffer-list-info-filter . 3) (anju-buffer-list-help-filter . 3)) #1# (alist :key-type (choice (function-item anju-buffer-list-project-filter) (function-item anju-buffer-list-plain-filter) (function-item anju-buffer-list-compilation-filter) (function-item anju-buffer-list-grep-filter) (function-item anju-buffer-list-xref-filter) (function-item anju-buffer-list-eshell-filter) (function-item anju-buffer-list-shell-filter) (function-item anju-buffer-list-info-filter) (function-item anju-buffer-list-help-filter) (function :tag "Custom buffer list filter")) :value-type integer) nil) (anju-reconfigure-main-menu-hook #2=(anju-main-menu--reconfigure-file anju-main-menu--reconfigure-edit anju-main-menu--reconfigure-registers anju-main-menu--reconfigure-options anju-main-menu--reconfigure-bookmarks anju-main-menu--reconfigure-text-mode anju-main-menu--reconfigure-tools anju-main-menu--reconfigure-help anju-main-menu--reconfigure-imenu) #2# hook nil) (anju-mode-line-buffer-list-function anju-buffer-list-menu-items anju-buffer-list-menu-items function nil)) (anju-context-menu-dired anju-context-menu-org-mode anju-context-menu-org-agenda anju-context-menu-info-mode anju-context-menu-make-mode anju-context-menu-compile anju-context-menu-elisp anju-context-menu-edebug-eval anju-context-menu-xref anju-context-menu-scratch anju-context-menu-buffers anju-context-menu-region anju-context-menu-dictionary anju-context-menu-narrow anju-context-menu-open-in anju-context-menu-vc anju-context-menu-markup anju-context-menu-wordcount anju-context-menu-rectangle anju-context-menu-window anju-context-menu-region-extension))"#
        ]],
    )
}

fn anju_autoload_registers_only_the_initialization_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_autoload_registers_only_the_initialization_command",
        r##"(list
         (featurep 'anju)
         (autoloadp (symbol-function 'anju-init))
         (commandp 'anju-init)
         (mapcar
          (lambda (symbol)
            (and (fboundp symbol) t))
          '(anju-style-bold
            anju-popup-buffer-menu
            anju-context-menu-org-mode)))"##,
        expect!["OK (nil t t (nil nil nil))"],
    )
}

pub(super) fn registry_anju_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anju_exact_pin_dependency_graph_and_features_match(),
        anju_public_and_internal_command_surface_is_callable(),
        anju_installed_payload_inventory_is_exact_and_unvendored(),
        anju_source_and_customization_inventory_matches(),
    ]
}

pub(super) fn registry_anju_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![anju_autoload_registers_only_the_initialization_command()]
}
