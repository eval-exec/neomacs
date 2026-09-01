use expect_test::expect;

use super::ParityBatchCase;

fn all_theme_face_registries_have_stable_complete_content_fingerprints() -> ParityBatchCase {
    ParityBatchCase::value(
        "all_theme_face_registries_have_stable_complete_content_fingerprints",
        r##"
(progn
  (mapc
   (lambda (theme)
     (load-theme theme t t))
   alabaster-themes-collection)
  (mapcar
   (lambda (theme)
     (let* ((settings
             (seq-filter
              (lambda (setting)
                (eq (car setting) 'theme-face))
              (get theme 'theme-settings)))
            (faces (mapcar #'cadr settings))
            duplicates)
       (dolist (face faces)
         (when (> (seq-count
                   (lambda (candidate)
                     (eq candidate face))
                   faces)
                  1)
           (cl-pushnew face duplicates)))
       (list
        theme
        (length settings)
        (length (delete-dups (copy-sequence faces)))
        (sort duplicates
              (lambda (left right)
                (string-lessp
                 (symbol-name left)
                 (symbol-name right))))
        (car faces)
        (car (last faces))
        (secure-hash
         'sha256
         (prin1-to-string
          (mapcar
           (lambda (setting)
             (secure-hash
              'sha256
              (prin1-to-string setting)))
           settings))))))
   alabaster-themes-collection))
"##,
        expect![[
            r#"OK ((alabaster-themes-light 501 501 nil default cursor "2fb6e2032be43146001b10a3ecda9d71ea8e8317bbac6e5da7fa6c33b118c063") (alabaster-themes-light-bg 103 89 (button cursor fringe highlight hl-line lazy-highlight line-number line-number-current-line link link-visited match minibuffer-prompt region tooltip) default cursor "b50f65c10a30b515edaec7bad3e5618ceda71c982441964c17b71e212bbfcb52") (alabaster-themes-light-mono 501 501 nil default cursor "f8bca7521d53526f775a578be4abfcfe6bb36f19491efa712b568af25b02095a") (alabaster-themes-dark 501 501 nil default cursor "f49821f8ba411793114c5c45214ab7e70a1e079240271cb5aecb9e9d1a4c0b46") (alabaster-themes-dark-mono 501 501 nil default cursor "bb3f48951d5ee13a5536ca3d98b85b5a043781598153f4c8cced993229ebbfec"))"#
        ]],
    )
}

fn core_ui_font_lock_and_diff_specs_are_exact_across_every_variant() -> ParityBatchCase {
    ParityBatchCase::value(
        "core_ui_font_lock_and_diff_specs_are_exact_across_every_variant",
        r##"
(progn
  (mapc
   (lambda (theme)
     (load-theme theme t t))
   alabaster-themes-collection)
  (let ((faces
         '(default cursor fringe line-number line-number-current-line
           hl-line region highlight isearch lazy-highlight match
           button link link-visited minibuffer-prompt
           error warning success shadow tooltip
           font-lock-comment-face font-lock-comment-delimiter-face
           font-lock-string-face font-lock-doc-face
           font-lock-keyword-face font-lock-builtin-face
           font-lock-function-name-face font-lock-variable-name-face
           font-lock-type-face font-lock-constant-face
           font-lock-preprocessor-face font-lock-warning-face
           mode-line mode-line-inactive mode-line-buffer-id
           diff-added diff-changed diff-removed
           diff-file-header diff-hunk-header)))
    (mapcar
     (lambda (theme)
       (cons
        theme
        (mapcar
         (lambda (face)
           (let ((entries
                  (seq-filter
                   (lambda (setting)
                     (eq (nth 1 setting) face))
                   (get theme 'theme-settings))))
             (list face
                   (mapcar
                    (lambda (entry)
                      (nth 3 entry))
                    entries))))
         faces)))
     alabaster-themes-collection)))
"##,
        expect![[
            r##"OK ((alabaster-themes-light (default (((#1=((class color) (min-colors 256)) :background "#F7F7F7" :foreground "#000000")))) (cursor (((#1# :background "#007acc")))) (fringe (((#1# :background "#f0f0f0" :foreground "#777777")))) (line-number (((#1# :inherit fringe)))) (line-number-current-line (((#1# :inherit bold :foreground "#000000")))) (hl-line (((#1# :background "#f0f0f0")))) (region (((#1# :background "#BFDBFE" :foreground "#000000")))) (highlight (((#1# :background "#BFDBFE" :foreground "#000000")))) (isearch (((#1# :background "#FFBC5D" :foreground "#000000")))) (lazy-highlight (((#1# :background "#FFFABC" :foreground "#000000")))) (match (((#1# :background "#DBF1FF")))) (button (((#1# :foreground "#325CC0" :underline "#cccccc")))) (link (((#1# :foreground "#325CC0" :underline "#cccccc")))) (link-visited (((#1# :foreground unspecified :underline "#cccccc")))) (minibuffer-prompt (((#1# :foreground "#325CC0")))) (error (((#1# :inherit bold :foreground "#AA3731")))) (warning (((#1# :inherit bold :foreground "#FFBC5D")))) (success (((#1# :inherit bold :foreground "#448C27")))) (shadow (((#1# :foreground "#777777")))) (tooltip (((#1# :background "#ffffff" :foreground "#000000")))) (font-lock-comment-face (((#1# :foreground "#AA3731")))) (font-lock-comment-delimiter-face (((#1# :inherit font-lock-comment-face)))) (font-lock-string-face (((#1# :foreground "#448C27")))) (font-lock-doc-face (((#1# :inherit font-lock-string-face)))) (font-lock-keyword-face (((#1# :foreground "#000000")))) (font-lock-builtin-face (((#1# :foreground "#AA3731")))) (font-lock-function-name-face (((#1# :foreground "#325CC0")))) (font-lock-variable-name-face (((#1# :foreground "#325CC0")))) (font-lock-type-face (((#1# :foreground "#000000")))) (font-lock-constant-face (((#1# :foreground "#7A3E9D")))) (font-lock-preprocessor-face (((#1# :foreground "#325CC0")))) (font-lock-warning-face (((#1# :inherit warning)))) (mode-line (((#1# :background "#e0e0e0" :foreground "#000000")))) (mode-line-inactive (((#1# :background "#f5f5f5" :foreground "#777777")))) (mode-line-buffer-id (((#1# :inherit bold)))) (diff-added (((#1# :background "#d4f6d4" :foreground "#005000")))) (diff-changed (((#1# :background "#ffe5b9" :foreground "#553d00")))) (diff-removed (((#1# :background "#ffd4d8" :foreground "#8f1313")))) (diff-file-header (((#1# :inherit bold :background "#ffffff")))) (diff-hunk-header (((#1# :background "#e0e0e0" :foreground "#000000"))))) (alabaster-themes-light-bg (default (((#1# :background "#ffffff" :foreground "#000000")))) (cursor (((#1# :background "#007acc")) ((#1# :background "#007acc")))) (fringe (((#1# :background "#f5f5f5" :foreground "#777777")) ((#1# :background "#f5f5f5" :foreground "#777777")))) (line-number (((#1# :inherit fringe)) ((#1# :inherit fringe)))) (line-number-current-line (((#1# :inherit bold :foreground "#000000")) ((#1# :inherit bold :foreground "#000000")))) (hl-line (((#1# :background "#f5f5f5")) ((#1# :background "#f5f5f5")))) (region (((#1# :background "#B4D8FD" :foreground "#000000")) ((#1# :background "#B4D8FD" :foreground "#000000")))) (highlight (((#1# :background "#BFDBFE" :foreground "#000000")) ((#1# :background "#BFDBFE" :foreground "#000000")))) (isearch (((#1# :background "#FFBC5D" :foreground "#000000")))) (lazy-highlight (((#1# :background "#FFFABC" :foreground "#000000")) ((#1# :background "#FFFABC" :foreground "#000000")))) (match (((#1# :background "#DBF1FF")) ((#1# :background "#DBF1FF")))) (button (((#1# :foreground "#325CC0" :underline "#cccccc")) ((#1# :foreground "#325CC0" :underline "#cccccc")))) (link (((#1# :foreground "#325CC0" :underline "#cccccc")) ((#1# :foreground "#325CC0" :underline "#cccccc")))) (link-visited (((#1# :foreground unspecified :underline "#cccccc")) ((#1# :foreground unspecified :underline "#cccccc")))) (minibuffer-prompt (((#1# :foreground "#325CC0")) ((#1# :foreground "#325CC0")))) (error (((#1# :inherit bold :foreground "#AA3731")))) (warning (((#1# :inherit bold :foreground "#FFBC5D")))) (success (((#1# :inherit bold :foreground "#448C27")))) (shadow (((#1# :foreground "#777777")))) (tooltip (((#1# :background "#fafafa" :foreground "#000000")) ((#1# :background "#fafafa" :foreground "#000000")))) (font-lock-comment-face (((#1# :background "#FFFABC" :foreground "#000000")))) (font-lock-comment-delimiter-face (((#1# :inherit font-lock-comment-face)))) (font-lock-string-face (((#1# :background "#F1FADF" :foreground "#000000")))) (font-lock-doc-face (((#1# :inherit font-lock-string-face)))) (font-lock-keyword-face (((#1# :foreground "#000000")))) (font-lock-builtin-face (((#1# :foreground "#AA3731")))) (font-lock-function-name-face (((#1# :background "#DBF1FF" :foreground "#000000")))) (font-lock-variable-name-face (((#1# :foreground "#325CC0")))) (font-lock-type-face (((#1# :foreground "#000000")))) (font-lock-constant-face (((#1# :background "#F9E0FF" :foreground "#000000")))) (font-lock-preprocessor-face (((#1# :foreground "#325CC0")))) (font-lock-warning-face (((#1# :inherit warning)))) (mode-line (((#1# :background "#e0e0e0" :foreground "#000000")))) (mode-line-inactive (((#1# :background "#f0f0f0" :foreground "#777777")))) (mode-line-buffer-id (((#1# :inherit bold)))) (diff-added (((#1# :background "#d4f6d4" :foreground "#005000")))) (diff-changed (((#1# :background "#ffe5b9" :foreground "#553d00")))) (diff-removed (((#1# :background "#ffd4d8" :foreground "#8f1313")))) (diff-file-header (((#1# :inherit bold :background "#fafafa")))) (diff-hunk-header (((#1# :background "#e0e0e0" :foreground "#000000"))))) (alabaster-themes-light-mono (default (((#1# :background "#F7F7F7" :foreground "#000000")))) (cursor (((#1# :background "#007acc")))) (fringe (((#1# :background "#f0f0f0" :foreground "#777777")))) (line-number (((#1# :inherit fringe)))) (line-number-current-line (((#1# :inherit bold :foreground "#000000")))) (hl-line (((#1# :background "#f0f0f0")))) (region (((#1# :background "#f0f0f0" :foreground "#000000")))) (highlight (((#1# :background "#f0f0f0" :foreground "#000000")))) (isearch (((#1# :background "#777777" :foreground "#000000")))) (lazy-highlight (((#1# :background "#f0f0f0" :foreground "#000000")))) (match (((#1# :background "#f0f0f0")))) (button (((#1# :foreground "#000000" :underline "#cccccc")))) (link (((#1# :foreground "#000000" :underline "#cccccc")))) (link-visited (((#1# :foreground unspecified :underline "#cccccc")))) (minibuffer-prompt (((#1# :foreground "#000000")))) (error (((#1# :inherit bold :foreground "#AA3731")))) (warning (((#1# :inherit bold :foreground "#FFBC5D")))) (success (((#1# :inherit bold :foreground "#000000")))) (shadow (((#1# :foreground "#777777")))) (tooltip (((#1# :background "#ffffff" :foreground "#000000")))) (font-lock-comment-face (((#1# :foreground "#777777")))) (font-lock-comment-delimiter-face (((#1# :inherit font-lock-comment-face)))) (font-lock-string-face (((#1# :foreground "#000000")))) (font-lock-doc-face (((#1# :inherit font-lock-string-face)))) (font-lock-keyword-face (((#1# :foreground "#000000")))) (font-lock-builtin-face (((#1# :foreground "#000000")))) (font-lock-function-name-face (((#1# :foreground "#000000")))) (font-lock-variable-name-face (((#1# :foreground "#000000")))) (font-lock-type-face (((#1# :foreground "#000000")))) (font-lock-constant-face (((#1# :foreground "#000000")))) (font-lock-preprocessor-face (((#1# :foreground "#000000")))) (font-lock-warning-face (((#1# :inherit warning)))) (mode-line (((#1# :background "#e0e0e0" :foreground "#000000")))) (mode-line-inactive (((#1# :background "#f5f5f5" :foreground "#777777")))) (mode-line-buffer-id (((#1# :inherit bold)))) (diff-added (((#1# :background "#d4f6d4" :foreground "#005000")))) (diff-changed (((#1# :background "#ffe5b9" :foreground "#553d00")))) (diff-removed (((#1# :background "#ffd4d8" :foreground "#8f1313")))) (diff-file-header (((#1# :inherit bold :background "#ffffff")))) (diff-hunk-header (((#1# :background "#e0e0e0" :foreground "#000000"))))) (alabaster-themes-dark (default (((#1# :background "#0E1415" :foreground "#CECECE")))) (cursor (((#1# :background "#CD974B")))) (fringe (((#1# :background "#1a1a1a" :foreground "#666666")))) (line-number (((#1# :inherit fringe)))) (line-number-current-line (((#1# :inherit bold :foreground "#ffffff")))) (hl-line (((#1# :background "#1a1a1a")))) (region (((#1# :background "#293334" :foreground "#CECECE")))) (highlight (((#1# :background "#293334" :foreground "#ffffff")))) (isearch (((#1# :background "#CD974B" :foreground "#ffffff")))) (lazy-highlight (((#1# :background "#332a20" :foreground "#ffffff")))) (match (((#1# :background "#202633")))) (button (((#1# :foreground "#8AB1F0" :underline "#444444")))) (link (((#1# :foreground "#8AB1F0" :underline "#444444")))) (link-visited (((#1# :foreground unspecified :underline "#444444")))) (minibuffer-prompt (((#1# :foreground "#8AB1F0")))) (error (((#1# :inherit bold :foreground "#DFDF8E")))) (warning (((#1# :inherit bold :foreground "#CD974B")))) (success (((#1# :inherit bold :foreground "#95CB82")))) (shadow (((#1# :foreground "#666666")))) (tooltip (((#1# :background "#1f2526" :foreground "#ffffff")))) (font-lock-comment-face (((#1# :foreground "#DFDF8E")))) (font-lock-comment-delimiter-face (((#1# :inherit font-lock-comment-face)))) (font-lock-string-face (((#1# :foreground "#95CB82")))) (font-lock-doc-face (((#1# :inherit font-lock-string-face)))) (font-lock-keyword-face (((#1# :foreground "#CECECE")))) (font-lock-builtin-face (((#1# :foreground "#DFDF8E")))) (font-lock-function-name-face (((#1# :foreground "#8AB1F0")))) (font-lock-variable-name-face (((#1# :foreground "#8AB1F0")))) (font-lock-type-face (((#1# :foreground "#CECECE")))) (font-lock-constant-face (((#1# :foreground "#CC8BC9")))) (font-lock-preprocessor-face (((#1# :foreground "#8AB1F0")))) (font-lock-warning-face (((#1# :inherit warning)))) (mode-line (((#1# :background "#293334" :foreground "#CECECE")))) (mode-line-inactive (((#1# :background "#121818" :foreground "#666666")))) (mode-line-buffer-id (((#1# :inherit bold)))) (diff-added (((#1# :background "#1f3a1f" :foreground "#95CB82")))) (diff-changed (((#1# :background "#3a2f1f" :foreground "#CD974B")))) (diff-removed (((#1# :background "#3a1f1f" :foreground "#ff6b6b")))) (diff-file-header (((#1# :inherit bold :background "#1f2526")))) (diff-hunk-header (((#1# :background "#293334" :foreground "#ffffff"))))) (alabaster-themes-dark-mono (default (((#1# :background "#0E1415" :foreground "#CECECE")))) (cursor (((#1# :background "#CD974B")))) (fringe (((#1# :background "#1a1a1a" :foreground "#666666")))) (line-number (((#1# :inherit fringe)))) (line-number-current-line (((#1# :inherit bold :foreground "#ffffff")))) (hl-line (((#1# :background "#1a1a1a")))) (region (((#1# :background "#1a1a1a" :foreground "#CECECE")))) (highlight (((#1# :background "#1a1a1a" :foreground "#ffffff")))) (isearch (((#1# :background "#666666" :foreground "#ffffff")))) (lazy-highlight (((#1# :background "#1a1a1a" :foreground "#ffffff")))) (match (((#1# :background "#1a1a1a")))) (button (((#1# :foreground "#CECECE" :underline "#444444")))) (link (((#1# :foreground "#CECECE" :underline "#444444")))) (link-visited (((#1# :foreground unspecified :underline "#444444")))) (minibuffer-prompt (((#1# :foreground "#CECECE")))) (error (((#1# :inherit bold :foreground "#ff6b6b")))) (warning (((#1# :inherit bold :foreground "#CD974B")))) (success (((#1# :inherit bold :foreground "#CECECE")))) (shadow (((#1# :foreground "#666666")))) (tooltip (((#1# :background "#1f2526" :foreground "#ffffff")))) (font-lock-comment-face (((#1# :foreground "#666666")))) (font-lock-comment-delimiter-face (((#1# :inherit font-lock-comment-face)))) (font-lock-string-face (((#1# :foreground "#CECECE")))) (font-lock-doc-face (((#1# :inherit font-lock-string-face)))) (font-lock-keyword-face (((#1# :foreground "#CECECE")))) (font-lock-builtin-face (((#1# :foreground "#CECECE")))) (font-lock-function-name-face (((#1# :foreground "#CECECE")))) (font-lock-variable-name-face (((#1# :foreground "#CECECE")))) (font-lock-type-face (((#1# :foreground "#CECECE")))) (font-lock-constant-face (((#1# :foreground "#CECECE")))) (font-lock-preprocessor-face (((#1# :foreground "#CECECE")))) (font-lock-warning-face (((#1# :inherit warning)))) (mode-line (((#1# :background "#293334" :foreground "#CECECE")))) (mode-line-inactive (((#1# :background "#121818" :foreground "#666666")))) (mode-line-buffer-id (((#1# :inherit bold)))) (diff-added (((#1# :background "#1f3a1f" :foreground "#95CB82")))) (diff-changed (((#1# :background "#3a2f1f" :foreground "#CD974B")))) (diff-removed (((#1# :background "#3a1f1f" :foreground "#ff6b6b")))) (diff-file-header (((#1# :inherit bold :background "#1f2526")))) (diff-hunk-header (((#1# :background "#293334" :foreground "#ffffff"))))))"##
        ]],
    )
}

fn representative_external_package_face_contracts_resolve_every_semantic_class() -> ParityBatchCase
{
    ParityBatchCase::value(
        "representative_external_package_face_contracts_resolve_every_semantic_class",
        r##"
(progn
  (load-theme 'alabaster-themes-light t t)
  (load-theme 'alabaster-themes-dark t t)
  (let ((faces
         '(magit-diff-added magit-diff-removed
           magit-diff-file-heading magit-section-heading
           org-document-title org-level-1 org-block org-todo
           dired-directory dired-flagged dired-marked
           company-tooltip company-tooltip-selection company-echo-common
           corfu-current corfu-quick1
           orderless-match-face-0 embark-selected
           flycheck-error flymake-warning
           rainbow-delimiters-depth-1-face
           rainbow-delimiters-mismatched-face
           message-header-subject gnus-summary-high-unread
           custom-group-tag custom-variable-tag)))
    (mapcar
     (lambda (theme)
       (cons
        theme
        (mapcar
         (lambda (face)
           (let ((entry
                  (seq-find
                   (lambda (setting)
                     (eq (nth 1 setting) face))
                   (get theme 'theme-settings))))
             (list face (copy-tree (nth 3 entry)))))
         faces)))
     '(alabaster-themes-light
       alabaster-themes-dark))))
"##,
        expect![[
            r##"OK ((alabaster-themes-light (magit-diff-added ((((class color) (min-colors 256)) :background "#e8fae8" :foreground "#005000"))) (magit-diff-removed ((((class color) (min-colors 256)) :background "#ffe3e3" :foreground "#8f1313"))) (magit-diff-file-heading ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0"))) (magit-section-heading ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0"))) (org-document-title ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0" :height 1.2))) (org-level-1 ((((class color) (min-colors 256)) :inherit bold :height unspecified :weight unspecified :foreground "#325CC0"))) (org-block ((((class color) (min-colors 256)) :background "#f5f5f5" :extend t))) (org-todo ((((class color) (min-colors 256)) :foreground "#FFBC5D"))) (dired-directory ((((class color) (min-colors 256)) :foreground "#325CC0"))) (dired-flagged ((((class color) (min-colors 256)) :inherit alabaster-themes-mark-delete))) (dired-marked ((((class color) (min-colors 256)) :inherit alabaster-themes-mark-select))) (company-tooltip ((((class color) (min-colors 256)) :background "#f5f5f5"))) (company-tooltip-selection ((((class color) (min-colors 256)) :background "#DBF1FF"))) (company-echo-common ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0"))) (corfu-current ((((class color) (min-colors 256)) :background "#DBF1FF"))) (corfu-quick1 ((((class color) (min-colors 256)) :inherit bold :background "#FFE0E0"))) (orderless-match-face-0 ((((class color) (min-colors 256)) :inherit bold :foreground unspecified))) (embark-selected ((((class color) (min-colors 256)) :inherit alabaster-themes-mark-select))) (flycheck-error ((((class color) (min-colors 256)) :inherit alabaster-themes-underline-error))) (flymake-warning ((((class color) (min-colors 256)) :inherit alabaster-themes-underline-warning))) (rainbow-delimiters-depth-1-face ((((class color) (min-colors 256)) :foreground unspecified))) (rainbow-delimiters-mismatched-face ((((class color) (min-colors 256)) :background "#ff6b6b" :foreground "#000000"))) (message-header-subject ((((class color) (min-colors 256)) :inherit bold :foreground "#000000"))) (gnus-summary-high-unread ((((class color) (min-colors 256)) :inherit bold :foreground "#000000"))) (custom-group-tag ((((class color) (min-colors 256)) :inherit bold :foreground "#AA3731"))) (custom-variable-tag ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0")))) (alabaster-themes-dark (magit-diff-added ((((class color) (min-colors 256)) :background "#2a4a2a" :foreground "#95CB82"))) (magit-diff-removed ((((class color) (min-colors 256)) :background "#4a2a2a" :foreground "#ff6b6b"))) (magit-diff-file-heading ((((class color) (min-colors 256)) :inherit bold :foreground "#8AB1F0"))) (magit-section-heading ((((class color) (min-colors 256)) :inherit bold :foreground "#8AB1F0"))) (org-document-title ((((class color) (min-colors 256)) :inherit bold :foreground "#8AB1F0" :height 1.2))) (org-level-1 ((((class color) (min-colors 256)) :inherit bold :height unspecified :weight unspecified :foreground "#8AB1F0"))) (org-block ((((class color) (min-colors 256)) :background "#121818" :extend t))) (org-todo ((((class color) (min-colors 256)) :foreground "#CD974B"))) (dired-directory ((((class color) (min-colors 256)) :foreground "#8AB1F0"))) (dired-flagged ((((class color) (min-colors 256)) :inherit alabaster-themes-mark-delete))) (dired-marked ((((class color) (min-colors 256)) :inherit alabaster-themes-mark-select))) (company-tooltip ((((class color) (min-colors 256)) :background "#121818"))) (company-tooltip-selection ((((class color) (min-colors 256)) :background "#202633"))) (company-echo-common ((((class color) (min-colors 256)) :inherit bold :foreground "#8AB1F0"))) (corfu-current ((((class color) (min-colors 256)) :background "#202633"))) (corfu-quick1 ((((class color) (min-colors 256)) :inherit bold :background "#332020"))) (orderless-match-face-0 ((((class color) (min-colors 256)) :inherit bold :foreground unspecified))) (embark-selected ((((class color) (min-colors 256)) :inherit alabaster-themes-mark-select))) (flycheck-error ((((class color) (min-colors 256)) :inherit alabaster-themes-underline-error))) (flymake-warning ((((class color) (min-colors 256)) :inherit alabaster-themes-underline-warning))) (rainbow-delimiters-depth-1-face ((((class color) (min-colors 256)) :foreground unspecified))) (rainbow-delimiters-mismatched-face ((((class color) (min-colors 256)) :background "#ff6b6b" :foreground "#ffffff"))) (message-header-subject ((((class color) (min-colors 256)) :inherit bold :foreground "#ffffff"))) (gnus-summary-high-unread ((((class color) (min-colors 256)) :inherit bold :foreground "#ffffff"))) (custom-group-tag ((((class color) (min-colors 256)) :inherit bold :foreground "#DFDF8E"))) (custom-variable-tag ((((class color) (min-colors 256)) :inherit bold :foreground "#8AB1F0")))))"##
        ]],
    )
}

fn no_bold_customization_rebuilds_actual_theme_specs_without_residual_inheritance()
-> ParityBatchCase {
    ParityBatchCase::value(
        "no_bold_customization_rebuilds_actual_theme_specs_without_residual_inheritance",
        r##"
(let ((faces
       '(error warning success mode-line-buffer-id
         diff-file-header magit-section-heading
         org-document-title dired-header
         company-echo-common orderless-match-face-0
         custom-group-tag message-header-subject)))
  (mapcar
   (lambda (no-bold)
     (let ((alabaster-themes-no-bold no-bold))
       (load-theme 'alabaster-themes-light t t)
       (list
        no-bold
        (mapcar
         (lambda (face)
           (let ((entry
                  (seq-find
                   (lambda (setting)
                     (eq (nth 1 setting) face))
                   (get 'alabaster-themes-light
                        'theme-settings))))
             (list face (copy-tree (nth 3 entry)))))
         faces)
        (alabaster-themes--bold))))
   '(nil t nil)))
"##,
        expect![[
            r##"OK ((nil ((error ((((class color) (min-colors 256)) :inherit bold :foreground "#AA3731"))) (warning ((((class color) (min-colors 256)) :inherit bold :foreground "#FFBC5D"))) (success ((((class color) (min-colors 256)) :inherit bold :foreground "#448C27"))) (mode-line-buffer-id ((((class color) (min-colors 256)) :inherit bold))) (diff-file-header ((((class color) (min-colors 256)) :inherit bold :background "#ffffff"))) (magit-section-heading ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0"))) (org-document-title ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0" :height 1.2))) (dired-header ((((class color) (min-colors 256)) :inherit bold))) (company-echo-common ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0"))) (orderless-match-face-0 ((((class color) (min-colors 256)) :inherit bold :foreground unspecified))) (custom-group-tag ((((class color) (min-colors 256)) :inherit bold :foreground "#AA3731"))) (message-header-subject ((((class color) (min-colors 256)) :inherit bold :foreground "#000000")))) (:inherit bold)) (t ((error ((((class color) (min-colors 256)) :foreground "#AA3731"))) (warning ((((class color) (min-colors 256)) :foreground "#FFBC5D"))) (success ((((class color) (min-colors 256)) :foreground "#448C27"))) (mode-line-buffer-id ((((class color) (min-colors 256))))) (diff-file-header ((((class color) (min-colors 256)) :background "#ffffff"))) (magit-section-heading ((((class color) (min-colors 256)) :foreground "#325CC0"))) (org-document-title ((((class color) (min-colors 256)) :foreground "#325CC0" :height 1.2))) (dired-header ((((class color) (min-colors 256))))) (company-echo-common ((((class color) (min-colors 256)) :foreground "#325CC0"))) (orderless-match-face-0 ((((class color) (min-colors 256)) :foreground unspecified))) (custom-group-tag ((((class color) (min-colors 256)) :foreground "#AA3731"))) (message-header-subject ((((class color) (min-colors 256)) :foreground "#000000")))) nil) (nil ((error ((((class color) (min-colors 256)) :inherit bold :foreground "#AA3731"))) (warning ((((class color) (min-colors 256)) :inherit bold :foreground "#FFBC5D"))) (success ((((class color) (min-colors 256)) :inherit bold :foreground "#448C27"))) (mode-line-buffer-id ((((class color) (min-colors 256)) :inherit bold))) (diff-file-header ((((class color) (min-colors 256)) :inherit bold :background "#ffffff"))) (magit-section-heading ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0"))) (org-document-title ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0" :height 1.2))) (dired-header ((((class color) (min-colors 256)) :inherit bold))) (company-echo-common ((((class color) (min-colors 256)) :inherit bold :foreground "#325CC0"))) (orderless-match-face-0 ((((class color) (min-colors 256)) :inherit bold :foreground unspecified))) (custom-group-tag ((((class color) (min-colors 256)) :inherit bold :foreground "#AA3731"))) (message-header-subject ((((class color) (min-colors 256)) :inherit bold :foreground "#000000")))) (:inherit bold)))"##
        ]],
    )
}

fn heading_customization_is_materialized_in_helper_and_org_face_specs_on_reload() -> ParityBatchCase
{
    ParityBatchCase::value(
        "heading_customization_is_materialized_in_helper_and_org_face_specs_on_reload",
        r##"
(let ((alabaster-themes-headings
       '((0 variable-pitch extrabold 1.8)
         (1 variable-pitch light 1.5)
         (2 semibold 1.3)
         (3 . t)
         (t variable-pitch 1.1)))
      (alabaster-themes-no-bold nil))
  (load-theme 'alabaster-themes-dark t t)
  (list
   (mapcar
    (lambda (level)
      (cons level
            (alabaster-themes--heading level)))
    '(0 1 2 3 4 8))
   (mapcar
    (lambda (face)
      (let ((entry
             (seq-find
              (lambda (setting)
                (eq (nth 1 setting) face))
              (get 'alabaster-themes-dark
                   'theme-settings))))
        (list face (nth 3 entry))))
    '(alabaster-themes-heading-0
      alabaster-themes-heading-1
      alabaster-themes-heading-2
      alabaster-themes-heading-3
      alabaster-themes-heading-4
      org-level-1 org-level-2 org-level-3 org-level-4))))
"##,
        expect![[
            r##"OK (((0 :inherit variable-pitch :height 1.8 :weight extrabold) (1 :inherit variable-pitch :height 1.5 :weight light) (2 :inherit nil :height 1.3 :weight semibold) (3 :inherit bold :height unspecified :weight unspecified) (4 :inherit (bold variable-pitch) :height 1.1 :weight unspecified) (8 :inherit (bold variable-pitch) :height 1.1 :weight unspecified)) ((alabaster-themes-heading-0 ((#1=((class color) (min-colors 256)) :inherit variable-pitch :height 1.8 :weight extrabold :foreground "#8AB1F0"))) (alabaster-themes-heading-1 ((#1# :inherit variable-pitch :height 1.5 :weight light :foreground "#8AB1F0"))) (alabaster-themes-heading-2 ((#1# :inherit nil :height 1.3 :weight semibold :foreground "#8AB1F0"))) (alabaster-themes-heading-3 ((#1# :inherit bold :height unspecified :weight unspecified :foreground "#8AB1F0"))) (alabaster-themes-heading-4 ((#1# :inherit (bold variable-pitch) :height 1.1 :weight unspecified :foreground "#8AB1F0"))) (org-level-1 ((#1# :inherit variable-pitch :height 1.5 :weight light :foreground "#8AB1F0"))) (org-level-2 ((#1# :inherit nil :height 1.3 :weight semibold :foreground "#8AB1F0"))) (org-level-3 ((#1# :inherit bold :height unspecified :weight unspecified :foreground "#8AB1F0"))) (org-level-4 ((#1# :inherit (bold variable-pitch) :height 1.1 :weight unspecified :foreground "#8AB1F0")))))"##
        ]],
    )
}

fn face_spec_selection_distinguishes_low_color_and_256_color_terminals_and_graphics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "face_spec_selection_distinguishes_low_color_and_256_color_terminals_and_graphics",
        r##"
(progn
  (load-theme 'alabaster-themes-light t t)
  (let* ((frame (selected-frame))
         (original-display-type
          (frame-parameter frame 'display-type))
         (settings
          (get 'alabaster-themes-light 'theme-settings))
         (spec
          (nth
           3
           (seq-find
            (lambda (setting)
              (eq (nth 1 setting) 'font-lock-string-face))
            settings))))
    (unwind-protect
        (progn
          (set-frame-parameter frame 'display-type 'color)
          (mapcar
           (lambda (case)
             (cl-letf
                 (((symbol-function 'display-color-cells)
                   (lambda (&optional _frame)
                     (nth 1 case)))
                  ((symbol-function 'window-system)
                   (lambda (&optional _frame)
                     (nth 0 case))))
               (list
                case
                (and
                 (face-spec-set-match-display
                  '((type tty)) frame)
                 t)
                (and
                 (face-spec-set-match-display
                  '((type graphic)) frame)
                 t)
                (face-spec-choose spec frame 'no-match))))
           '((nil 16)
             (nil 256)
             (x 256)
             (pgtk 16777216))))
      (set-frame-parameter
       frame 'display-type original-display-type))))
"##,
        expect![[
            r##"OK (((nil 16) t nil no-match) ((nil 256) t nil #1=(:foreground "#448C27")) ((x 256) nil t #1#) ((pgtk 16777216) nil t #1#))"##
        ]],
    )
}

fn enabled_theme_resolves_inheritance_and_literal_attributes_for_builtin_faces() -> ParityBatchCase
{
    ParityBatchCase::value(
        "enabled_theme_resolves_inheritance_and_literal_attributes_for_builtin_faces",
        r##"
(progn
  (require 'diff-mode)
  (require 'flymake)
  (require 'org)
  (mapc #'disable-theme custom-enabled-themes)
  (unwind-protect
      (progn
        (load-theme 'alabaster-themes-dark t)
        (mapcar
         (lambda (face)
           (list
            face
            (face-attribute face :foreground nil 'default)
            (face-attribute face :background nil 'default)
            (face-attribute face :inherit nil 'default)
            (face-attribute face :weight nil 'default)
            (face-attribute face :underline nil 'default)))
         '(default fringe line-number
           font-lock-comment-delimiter-face
           font-lock-doc-face font-lock-warning-face
           diff-indicator-added
           flymake-error
           org-level-1
           mode-line-highlight)))
    (mapc #'disable-theme custom-enabled-themes)))
"##,
        expect![[
            r#"OK ((default "unspecified-fg" "unspecified-bg" nil normal nil) (fringe "unspecified-fg" "gray" nil normal nil) (line-number "unspecified-fg" "unspecified-bg" (shadow default) normal nil) (font-lock-comment-delimiter-face "unspecified-fg" "unspecified-bg" font-lock-comment-face bold nil) (font-lock-doc-face "unspecified-fg" "unspecified-bg" font-lock-string-face normal nil) (font-lock-warning-face "unspecified-fg" "unspecified-bg" error bold nil) (diff-indicator-added "unspecified-fg" "unspecified-bg" diff-added normal nil) (flymake-error "unspecified-fg" "unspecified-bg" error bold nil) (org-level-1 "unspecified-fg" "unspecified-bg" outline-1 bold nil) (mode-line-highlight "unspecified-fg" "unspecified-bg" highlight normal nil))"#
        ]],
    )
}

pub(super) fn faces_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        all_theme_face_registries_have_stable_complete_content_fingerprints(),
        core_ui_font_lock_and_diff_specs_are_exact_across_every_variant(),
        representative_external_package_face_contracts_resolve_every_semantic_class(),
        no_bold_customization_rebuilds_actual_theme_specs_without_residual_inheritance(),
        heading_customization_is_materialized_in_helper_and_org_face_specs_on_reload(),
        face_spec_selection_distinguishes_low_color_and_256_color_terminals_and_graphics(),
        enabled_theme_resolves_inheritance_and_literal_attributes_for_builtin_faces(),
    ]
}
