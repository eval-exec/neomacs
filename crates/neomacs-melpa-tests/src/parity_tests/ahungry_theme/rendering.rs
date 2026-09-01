use expect_test::expect;

use super::ParityBatchCase;

fn theme_loaded_before_org_styles_a_real_release_document_with_visible_semantics() -> ParityBatchCase
{
    ParityBatchCase::value(
        "theme_loaded_before_org_styles_a_real_release_document_with_visible_semantics",
        r##"(let ((theme 'ahungry))
  (unwind-protect
      (progn
        (enable-theme theme)
        (require 'org)
        (with-temp-buffer
          (org-mode)
          (insert
           "#+title: Release Notes\n"
           "* TODO Ship [[https://example.test/docs][documentation]]\n"
           "SCHEDULED: <2026-07-28 Tue>\n"
           ":PROPERTIES:\n"
           ":Owner: Ada\n"
           ":END:\n"
           "Run =cargo nextest= before rollout.\n"
           "#+begin_quote\n"
           "Safe rollout\n"
           "#+end_quote\n"
           "#+begin_src emacs-lisp\n"
           "(message \"ready\")\n"
           "#+end_src\n"
           "| Item | State |\n"
           "| API  | Ready |\n"
           "* DONE Archive release\n")
          (font-lock-ensure)
          (let ((describe
                 (lambda (token)
                   (goto-char (point-min))
                   (search-forward token)
                   (let* ((position
                           (- (point) (length token)))
                          (face
                           (get-text-property
                            position 'face))
                          (primary
                           (if (listp face)
                               (car face)
                             face)))
                     (list
                      token
                      face
                      (and
                       primary
                       (face-attribute
                        primary :background nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :foreground nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :weight nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :slant nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :underline nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :height nil 'default)))))))
            (list
             major-mode
             (copy-sequence custom-enabled-themes)
             (face-attribute
              'default :background nil 'default)
             (face-attribute
              'org-hide :foreground nil 'default)
             (mapcar
              describe
              '("#+title:"
                "Release Notes"
                "TODO"
                "Ship"
                "documentation"
                "<2026-07-28 Tue>"
                ":PROPERTIES:"
                ":Owner:"
                "Ada"
                "cargo nextest"
                "#+begin_quote"
                "#+begin_src"
                "(message \"ready\")"
                "| Item | State |"
                "DONE"
                "Archive release"))))))
    (when (memq theme custom-enabled-themes)
      (disable-theme theme))))"##,
        expect![[
            r##"OK (org-mode (ahungry) unspecified "#222222" (("#+title:" org-document-info-keyword unspecified "#aaaaaa" normal normal nil 130) ("Release Notes" org-document-title unspecified "#0077cc" bold normal nil 130) ("TODO" (org-todo org-level-1) unspecified "#ff0099" bold normal nil 130) ("Ship" org-level-1 unspecified "#4477ff" bold normal nil 182) ("documentation" (org-link org-level-1) "#111111" "#ff0099" normal normal nil 130) ("<2026-07-28 Tue>" (org-date) unspecified "#ff0066" normal normal t 130) (":PROPERTIES:" org-drawer unspecified "#ffffff" bold normal nil 130) (":Owner:" org-special-keyword unspecified "#cc0033" normal normal nil 130) ("Ada" org-property-value unspecified "#ffffff" normal normal nil 130) ("cargo nextest" (org-verbatim) unspecified "#cc6600" normal italic t 130) ("#+begin_quote" org-block-begin-line "#333333" "#bbbbbb" normal normal nil 130) ("#+begin_src" org-block-begin-line "#333333" "#bbbbbb" normal normal nil 130) ("(message \"ready\")" (org-block) unspecified "#999999" normal normal nil 130) ("| Item | State |" org-table unspecified "#0055ff" normal normal nil 130) ("DONE" (org-done org-level-1) unspecified "#00cc33" bold normal nil 130) ("Archive release" (org-headline-done org-level-1) unspecified "#ffffff" normal normal nil 130)))"##
        ]],
    )
}

fn real_unified_diff_resolves_file_hunk_context_removed_and_added_semantics() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_unified_diff_resolves_file_hunk_context_removed_and_added_semantics",
        r##"(let ((theme 'ahungry))
  (unwind-protect
      (progn
        (enable-theme theme)
        (with-temp-buffer
          (diff-mode)
          (insert
           "diff --git a/src/lib.rs b/src/lib.rs\n"
           "index 56a6051..f47c63d 100644\n"
           "--- a/src/lib.rs\n"
           "+++ b/src/lib.rs\n"
           "@@ -1,3 +1,3 @@\n"
           " fn value() -> i32 {\n"
           "-    old_value\n"
           "+    new_value\n"
           " }\n")
          (font-lock-ensure)
          (let ((describe
                 (lambda (token)
                   (goto-char (point-min))
                   (search-forward token)
                   (let* ((position
                           (- (point) (length token)))
                          (face
                           (get-text-property
                            position 'face))
                          (primary
                           (if (listp face)
                               (car face)
                             face)))
                     (list
                      token
                      face
                      (and
                       primary
                       (face-attribute
                        primary :background nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :foreground nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :weight nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :slant nil 'default)))))))
            (list
             major-mode
             (copy-sequence custom-enabled-themes)
             (mapcar
              describe
              '("diff --git a/src/lib.rs b/src/lib.rs"
                "index 56a6051..f47c63d 100644"
                "--- a/src/lib.rs"
                "+++ b/src/lib.rs"
                "@@ -1,3 +1,3 @@"
                "fn value() -> i32 {"
                "-    old_value"
                "old_value"
                "+    new_value"
                "new_value"))))))
    (when (memq theme custom-enabled-themes)
      (disable-theme theme))))"##,
        expect![[
            r##"OK (diff-mode (ahungry) (("diff --git a/src/lib.rs b/src/lib.rs" diff-header "#444444" "#ffffff" normal normal) ("index 56a6051..f47c63d 100644" diff-header "#444444" "#ffffff" normal normal) ("--- a/src/lib.rs" diff-header "#444444" "#ffffff" normal normal) ("+++ b/src/lib.rs" diff-header "#444444" "#ffffff" normal normal) ("@@ -1,3 +1,3 @@" diff-hunk-header unspecified "#ffff00" normal normal) ("fn value() -> i32 {" diff-context unspecified "#777777" normal normal) ("-    old_value" diff-indicator-removed "default" "#ff0000" normal normal) ("old_value" diff-removed "default" "#ff0000" normal normal) ("+    new_value" diff-indicator-added "default" "#00ff00" normal normal) ("new_value" diff-added "default" "#00ff00" normal normal)))"##
        ]],
    )
}

fn completion_search_and_navigation_ui_resolve_core_and_extension_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "completion_search_and_navigation_ui_resolve_core_and_extension_faces",
        r##"(let ((theme 'ahungry)
      (extension-faces
       '(helm-selection
         helm-match
         isearch-lazy-highlight-face)))
  (unwind-protect
      (progn
        (dolist (face extension-faces)
          (unless (facep face)
            (make-face face)))
        (enable-theme theme)
        (with-temp-buffer
          (completion-list-mode)
          (let ((inhibit-read-only t))
            (dolist
                (entry
                 '(("Choose command: " minibuffer-prompt)
                   ("ship-release" helm-selection)
                   ("matching fragment" helm-match)
                   ("selected text" region)
                   ("exact match" match)
                   ("active search" isearch)
                   ("lazy search" isearch-lazy-highlight-face)
                   ("documentation" link)
                   ("open details" button)
                   ("fatal problem" error)))
              (insert
               (propertize
                (car entry) 'face (cadr entry))
               "\n")))
          (let ((describe
                 (lambda (token)
                   (goto-char (point-min))
                   (search-forward token)
                   (let* ((position
                           (- (point) (length token)))
                          (face
                           (get-text-property
                            position 'face))
                          (primary
                           (if (listp face)
                               (car face)
                             face)))
                     (list
                      token
                      face
                      (and
                       primary
                       (face-attribute
                        primary :background nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :foreground nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :weight nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :slant nil 'default))
                      (and
                       primary
                       (face-attribute
                        primary :underline nil 'default)))))))
            (list
             major-mode
             (copy-sequence custom-enabled-themes)
             (mapcar
              describe
              '("Choose command: "
                "ship-release"
                "matching fragment"
                "selected text"
                "exact match"
                "active search"
                "lazy search"
                "documentation"
                "open details"
                "fatal problem"))
             (list
              (face-attribute
               'mode-line :background nil 'default)
              (face-attribute
               'mode-line :foreground nil 'default)
              (copy-tree
               (face-attribute
                'mode-line :box nil 'default)))
             (list
              (face-attribute
               'tooltip :background nil 'default)
              (face-attribute
               'tooltip :foreground nil 'default)
              (face-attribute
               'tooltip :inherit nil 'default))
             (face-attribute
              'fringe :background nil 'default)))))
    (when (memq theme custom-enabled-themes)
      (disable-theme theme))))"##,
        expect![[
            r##"OK (completion-list-mode (ahungry) (("Choose command: " minibuffer-prompt unspecified "#0055ff" bold normal nil) ("ship-release" helm-selection unspecified "#cf0066" bold normal nil) ("matching fragment" helm-match unspecified "gold1" normal normal nil) ("selected text" region "#444444" "#ffffff" normal normal nil) ("exact match" match "#e9b96e" "#2e3436" bold normal nil) ("active search" isearch "#ff6600" "#333333" normal normal nil) ("lazy search" isearch-lazy-highlight-face "#ff6600" "#2e3436" normal normal nil) ("documentation" link unspecified "#33ff99" normal normal t) ("open details" button unspecified "#0055ff" bold normal t) ("fatal problem" error unspecified "Red1" bold normal nil)) ("#77ff00" "#0022aa" (:line-width 1 :color nil :style released-button)) ("#ffff33" "black" 'variable-pitch) "#333333")"##
        ]],
    )
}

pub(super) fn rendering_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        theme_loaded_before_org_styles_a_real_release_document_with_visible_semantics(),
        real_unified_diff_resolves_file_hunk_context_removed_and_added_semantics(),
        completion_search_and_navigation_ui_resolve_core_and_extension_faces(),
    ]
}
