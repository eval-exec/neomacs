use expect_test::expect;

use super::ParityBatchCase;

fn real_emacs_lisp_font_lock_exercises_all_five_theme_highlighting_philosophies() -> ParityBatchCase
{
    ParityBatchCase::value(
        "real_emacs_lisp_font_lock_exercises_all_five_theme_highlighting_philosophies",
        r##"
(progn
  (mapc #'disable-theme custom-enabled-themes)
  (unwind-protect
      (mapcar
       (lambda (theme)
         (alabaster-themes-load-theme theme)
         (with-temp-buffer
           (emacs-lisp-mode)
           (insert
            "(defun settle-invoice (invoice)\n\
  \"Return paid status for INVOICE.\"\n\
  ;; Explain the settlement decision.\n\
  (let ((status :paid))\n\
    (when invoice\n\
      (message \"paid\"))\n\
    status))\n")
           (font-lock-ensure)
           (cons
            theme
            (mapcar
             (lambda (token)
               (goto-char (point-min))
               (search-forward token)
               (let* ((start
                       (- (point) (length token)))
                      (face
                       (get-text-property start 'face)))
                 (list
                  token face
                  (and (facep face)
                       (face-attribute
                        face :foreground nil 'default))
                  (and (facep face)
                       (face-attribute
                        face :background nil 'default))
                  (and (facep face)
                       (face-attribute
                        face :weight nil 'default)))))
             '("defun" "settle-invoice"
               "Return paid status" "Explain"
               "let" ":paid" "when" "message"
               "\"paid\"")))))
       alabaster-themes-collection)
    (mapc #'disable-theme custom-enabled-themes)))
"##,
        expect![[
            r#"OK ((alabaster-themes-light ("defun" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("settle-invoice" font-lock-function-name-face "unspecified-fg" "unspecified-bg" bold) ("Return paid status" font-lock-doc-face "unspecified-fg" "unspecified-bg" normal) ("Explain" font-lock-comment-face "unspecified-fg" "unspecified-bg" bold) ("let" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) (":paid" font-lock-builtin-face "unspecified-fg" "unspecified-bg" bold) ("when" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("message" nil nil nil nil) ("\"paid\"" font-lock-string-face "unspecified-fg" "unspecified-bg" normal)) (alabaster-themes-light-bg ("defun" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("settle-invoice" font-lock-function-name-face "unspecified-fg" "unspecified-bg" bold) ("Return paid status" font-lock-doc-face "unspecified-fg" "unspecified-bg" normal) ("Explain" font-lock-comment-face "unspecified-fg" "unspecified-bg" bold) ("let" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) (":paid" font-lock-builtin-face "unspecified-fg" "unspecified-bg" bold) ("when" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("message" nil nil nil nil) ("\"paid\"" font-lock-string-face "unspecified-fg" "unspecified-bg" normal)) (alabaster-themes-light-mono ("defun" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("settle-invoice" font-lock-function-name-face "unspecified-fg" "unspecified-bg" bold) ("Return paid status" font-lock-doc-face "unspecified-fg" "unspecified-bg" normal) ("Explain" font-lock-comment-face "unspecified-fg" "unspecified-bg" bold) ("let" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) (":paid" font-lock-builtin-face "unspecified-fg" "unspecified-bg" bold) ("when" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("message" nil nil nil nil) ("\"paid\"" font-lock-string-face "unspecified-fg" "unspecified-bg" normal)) (alabaster-themes-dark ("defun" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("settle-invoice" font-lock-function-name-face "unspecified-fg" "unspecified-bg" bold) ("Return paid status" font-lock-doc-face "unspecified-fg" "unspecified-bg" normal) ("Explain" font-lock-comment-face "unspecified-fg" "unspecified-bg" bold) ("let" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) (":paid" font-lock-builtin-face "unspecified-fg" "unspecified-bg" bold) ("when" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("message" nil nil nil nil) ("\"paid\"" font-lock-string-face "unspecified-fg" "unspecified-bg" normal)) (alabaster-themes-dark-mono ("defun" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("settle-invoice" font-lock-function-name-face "unspecified-fg" "unspecified-bg" bold) ("Return paid status" font-lock-doc-face "unspecified-fg" "unspecified-bg" normal) ("Explain" font-lock-comment-face "unspecified-fg" "unspecified-bg" bold) ("let" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) (":paid" font-lock-builtin-face "unspecified-fg" "unspecified-bg" bold) ("when" font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("message" nil nil nil nil) ("\"paid\"" font-lock-string-face "unspecified-fg" "unspecified-bg" normal)))"#
        ]],
    )
}

fn real_org_document_resolves_titles_todos_links_blocks_and_metadata_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_org_document_resolves_titles_todos_links_blocks_and_metadata_faces",
        r##"
(progn
  (require 'org)
  (mapc #'disable-theme custom-enabled-themes)
  (unwind-protect
      (mapcar
       (lambda (theme)
         (alabaster-themes-load-theme theme)
         (with-temp-buffer
           (org-mode)
           (insert
            "#+title: Settlement Runbook\n\
* TODO Validate transaction\n\
See [[https://example.invalid][ledger documentation]].\n\
#+begin_src emacs-lisp\n\
(message \"validate\")\n\
#+end_src\n")
           (font-lock-ensure)
           (cons
            theme
            (mapcar
             (lambda (token)
               (goto-char (point-min))
               (search-forward token)
               (let* ((start
                       (- (point) (length token)))
                      (face
                       (get-text-property start 'face))
                      (primary
                       (if (consp face) (car face) face)))
                 (list
                  token face
                  (and (facep primary)
                       (face-attribute
                        primary :foreground nil 'default))
                  (and (facep primary)
                       (face-attribute
                        primary :background nil 'default))
                  (and (facep primary)
                       (face-attribute
                        primary :inherit nil 'default)))))
             '("#+title:" "Settlement Runbook"
               "TODO" "Validate transaction"
               "https://example.invalid"
               "ledger documentation"
               "#+begin_src" "message"
               "#+end_src")))))
       '(alabaster-themes-light
         alabaster-themes-dark
         alabaster-themes-light-mono))
    (mapc #'disable-theme custom-enabled-themes)))
"##,
        expect![[
            r##"OK ((alabaster-themes-light ("#+title:" org-document-info-keyword "unspecified-fg" "unspecified-bg" shadow) ("Settlement Runbook" org-document-title "unspecified-fg" "unspecified-bg" nil) ("TODO" (org-todo org-level-1) "unspecified-fg" "unspecified-bg" nil) ("Validate transaction" org-level-1 "unspecified-fg" "unspecified-bg" outline-1) ("https://example.invalid" org-link "unspecified-fg" "unspecified-bg" link) ("ledger documentation" org-link "unspecified-fg" "unspecified-bg" link) ("#+begin_src" org-block-begin-line "unspecified-fg" "unspecified-bg" org-meta-line) ("message" (org-block) "unspecified-fg" "unspecified-bg" shadow) ("#+end_src" org-block-end-line "unspecified-fg" "unspecified-bg" org-block-begin-line)) (alabaster-themes-dark ("#+title:" org-document-info-keyword "unspecified-fg" "unspecified-bg" shadow) ("Settlement Runbook" org-document-title "unspecified-fg" "unspecified-bg" nil) ("TODO" (org-todo org-level-1) "unspecified-fg" "unspecified-bg" nil) ("Validate transaction" org-level-1 "unspecified-fg" "unspecified-bg" outline-1) ("https://example.invalid" org-link "unspecified-fg" "unspecified-bg" link) ("ledger documentation" org-link "unspecified-fg" "unspecified-bg" link) ("#+begin_src" org-block-begin-line "unspecified-fg" "unspecified-bg" org-meta-line) ("message" (org-block) "unspecified-fg" "unspecified-bg" shadow) ("#+end_src" org-block-end-line "unspecified-fg" "unspecified-bg" org-block-begin-line)) (alabaster-themes-light-mono ("#+title:" org-document-info-keyword "unspecified-fg" "unspecified-bg" shadow) ("Settlement Runbook" org-document-title "unspecified-fg" "unspecified-bg" nil) ("TODO" (org-todo org-level-1) "unspecified-fg" "unspecified-bg" nil) ("Validate transaction" org-level-1 "unspecified-fg" "unspecified-bg" outline-1) ("https://example.invalid" org-link "unspecified-fg" "unspecified-bg" link) ("ledger documentation" org-link "unspecified-fg" "unspecified-bg" link) ("#+begin_src" org-block-begin-line "unspecified-fg" "unspecified-bg" org-meta-line) ("message" (org-block) "unspecified-fg" "unspecified-bg" shadow) ("#+end_src" org-block-end-line "unspecified-fg" "unspecified-bg" org-block-begin-line)))"##
        ]],
    )
}

fn real_diff_buffer_applies_file_hunk_added_removed_context_and_refined_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_diff_buffer_applies_file_hunk_added_removed_context_and_refined_faces",
        r##"
(progn
  (require 'diff-mode)
  (mapc #'disable-theme custom-enabled-themes)
  (unwind-protect
      (mapcar
       (lambda (theme)
         (alabaster-themes-load-theme theme)
         (with-temp-buffer
           (insert
            "diff --git a/payment.ak b/payment.ak\n\
--- a/payment.ak\n\
+++ b/payment.ak\n\
@@ -1,3 +1,3 @@\n\
-const amount = 10\n\
+const amount = 20\n\
 context line\n")
           (diff-mode)
           (font-lock-ensure)
           (cons
            theme
            (mapcar
             (lambda (token)
               (goto-char (point-min))
               (search-forward token)
               (let* ((start
                       (- (point) (length token)))
                      (face
                       (get-text-property start 'face))
                      (primary
                       (if (consp face) (car face) face)))
                 (list
                  token face
                  (and (facep primary)
                       (face-attribute
                        primary :foreground nil 'default))
                  (and (facep primary)
                       (face-attribute
                        primary :background nil 'default)))))
             '("diff --git" "--- a/payment.ak"
               "+++ b/payment.ak" "@@ -1,3"
               "-const amount" "+const amount"
               "context line")))))
       '(alabaster-themes-light
         alabaster-themes-light-bg
         alabaster-themes-dark))
    (mapc #'disable-theme custom-enabled-themes)))
"##,
        expect![[
            r#"OK ((alabaster-themes-light ("diff --git" diff-header "unspecified-fg" "unspecified-bg") ("--- a/payment.ak" diff-header "unspecified-fg" "unspecified-bg") ("+++ b/payment.ak" diff-header "unspecified-fg" "unspecified-bg") ("@@ -1,3" diff-hunk-header "unspecified-fg" "unspecified-bg") ("-const amount" diff-indicator-removed "unspecified-fg" "unspecified-bg") ("+const amount" diff-indicator-added "unspecified-fg" "unspecified-bg") ("context line" diff-context "unspecified-fg" "unspecified-bg")) (alabaster-themes-light-bg ("diff --git" diff-header "unspecified-fg" "unspecified-bg") ("--- a/payment.ak" diff-header "unspecified-fg" "unspecified-bg") ("+++ b/payment.ak" diff-header "unspecified-fg" "unspecified-bg") ("@@ -1,3" diff-hunk-header "unspecified-fg" "unspecified-bg") ("-const amount" diff-indicator-removed "unspecified-fg" "unspecified-bg") ("+const amount" diff-indicator-added "unspecified-fg" "unspecified-bg") ("context line" diff-context "unspecified-fg" "unspecified-bg")) (alabaster-themes-dark ("diff --git" diff-header "unspecified-fg" "unspecified-bg") ("--- a/payment.ak" diff-header "unspecified-fg" "unspecified-bg") ("+++ b/payment.ak" diff-header "unspecified-fg" "unspecified-bg") ("@@ -1,3" diff-hunk-header "unspecified-fg" "unspecified-bg") ("-const amount" diff-indicator-removed "unspecified-fg" "unspecified-bg") ("+const amount" diff-indicator-added "unspecified-fg" "unspecified-bg") ("context line" diff-context "unspecified-fg" "unspecified-bg")))"#
        ]],
    )
}

fn real_compilation_buffer_preserves_diagnostic_faces_and_location_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_compilation_buffer_preserves_diagnostic_faces_and_location_metadata",
        r##"
(progn
  (require 'compile)
  (mapc #'disable-theme custom-enabled-themes)
  (unwind-protect
      (mapcar
       (lambda (theme)
         (alabaster-themes-load-theme theme)
         (with-temp-buffer
           (compilation-mode)
           (let ((inhibit-read-only t))
             (insert
              "Checking payment\n\
src/payment.ak:3:5: error: invalid datum\n\
src/payment.ak:8:2: warning: unused redeemer\n"))
           (font-lock-ensure)
           (cons
            theme
            (mapcar
             (lambda (token)
               (goto-char (point-min))
               (search-forward token)
               (let* ((start
                       (- (point) (length token)))
                      (face
                       (get-text-property start 'face))
                      (message
                       (get-text-property
                        start 'compilation-message)))
                 (list
                  token face (and message t)
                  (and (facep face)
                       (face-attribute
                        face :foreground nil 'default))
                  (and (facep face)
                       (face-attribute
                        face :inherit nil 'default)))))
             '("src/payment.ak:3:5"
               "error"
               "src/payment.ak:8:2"
               "warning")))))
       '(alabaster-themes-light
         alabaster-themes-dark))
    (mapc #'disable-theme custom-enabled-themes)))
"##,
        expect![[
            r#"OK ((alabaster-themes-light ("src/payment.ak:3:5" font-lock-function-name-face t "unspecified-fg" nil) ("error" nil t nil nil) ("src/payment.ak:8:2" font-lock-function-name-face t "unspecified-fg" nil) ("warning" nil t nil nil)) (alabaster-themes-dark ("src/payment.ak:3:5" font-lock-function-name-face t "unspecified-fg" nil) ("error" nil t nil nil) ("src/payment.ak:8:2" font-lock-function-name-face t "unspecified-fg" nil) ("warning" nil t nil nil)))"#
        ]],
    )
}

fn real_dired_listing_applies_directory_symlink_mark_and_flag_workflow_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_dired_listing_applies_directory_symlink_mark_and_flag_workflow_faces",
        r##"
(progn
  (require 'dired)
  (mapc #'disable-theme custom-enabled-themes)
  (let* ((root (make-temp-file "alabaster-dired-" t))
         (directory
          (expand-file-name "contracts" root))
         (file
          (expand-file-name "payment.ak" root))
         (link
          (expand-file-name "payment-link.ak" root))
         buffer)
    (unwind-protect
        (progn
          (make-directory directory)
          (with-temp-file file
            (insert "validator payment {}\n"))
          (make-symbolic-link file link)
          (alabaster-themes-load-theme
           'alabaster-themes-light)
          (setq buffer
                (dired-noselect
                 (file-name-as-directory root)
                 "-al"))
          (with-current-buffer buffer
            (font-lock-ensure)
            (goto-char (point-min))
            (dired-goto-file file)
            (dired-mark 1)
            (font-lock-flush)
            (font-lock-ensure)
            (mapcar
             (lambda (token)
               (goto-char (point-min))
               (search-forward token)
               (let* ((start
                       (- (point) (length token)))
                      (face
                       (get-text-property start 'face))
                      (primary
                       (if (consp face) (car face) face)))
                 (list
                  token face
                  (and (facep primary)
                       (face-attribute
                        primary :foreground nil 'default))
                  (and (facep primary)
                       (face-attribute
                        primary :background nil 'default))
                  (and (facep primary)
                       (face-attribute
                        primary :inherit nil 'default)))))
             '("contracts" "payment.ak"
               "payment-link.ak"))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer))
      (mapc #'disable-theme custom-enabled-themes)
      (delete-directory root t))))
"##,
        expect![[
            r#"OK (("contracts" dired-directory "unspecified-fg" "unspecified-bg" font-lock-function-name-face) ("payment.ak" default "unspecified-fg" "unspecified-bg" nil) ("payment-link.ak" dired-symlink "unspecified-fg" "unspecified-bg" font-lock-keyword-face))"#
        ]],
    )
}

fn semantic_terminal_palette_drives_real_ansi_color_rendering_for_each_variant() -> ParityBatchCase
{
    ParityBatchCase::value(
        "semantic_terminal_palette_drives_real_ansi_color_rendering_for_each_variant",
        r##"
(progn
  (require 'ansi-color)
  (mapc #'disable-theme custom-enabled-themes)
  (unwind-protect
      (mapcar
       (lambda (theme)
         (alabaster-themes-load-theme theme)
         (let* ((ansi-color-names-vector
                 (alabaster-themes-with-colors
                   (vector
                    fg-term-black fg-term-red
                    fg-term-green fg-term-yellow
                    fg-term-blue fg-term-magenta
                    fg-term-cyan fg-term-white)))
                (rendered
                 (ansi-color-apply
                  "\e[31mred\e[0m \
\e[32mgreen\e[0m \
\e[34mblue\e[0m \
\e[35mmagenta\e[0m")))
           (list
            theme
            (append ansi-color-names-vector nil)
            (substring-no-properties rendered)
            (mapcar
             (lambda (index)
               (list
                index
                (aref rendered index)
                (text-properties-at index rendered)))
             '(0 2 3 4 8 9 10 13 14 15 20 21)))))
       alabaster-themes-collection)
    (mapc #'disable-theme custom-enabled-themes)))
"##,
        expect![[
            r##"OK ((alabaster-themes-light ("black" "#AA3731" "#448C27" "#FFBC5D" "#325CC0" "#7A3E9D" "#325CC0" "gray65") "red green blue magenta" ((0 114 #1=(font-lock-face (:foreground "red3"))) (2 100 #1#) (3 32 nil) (4 103 #2=(font-lock-face (:foreground "green3"))) (8 110 #2#) (9 32 nil) (10 98 #3=(font-lock-face (:foreground "blue2"))) (13 101 #3#) (14 32 nil) (15 109 #4=(font-lock-face (:foreground "magenta3"))) (20 116 #4#) (21 97 #4#))) (alabaster-themes-light-bg ("black" "#AA3731" "#448C27" "#FFBC5D" "#325CC0" "#7A3E9D" "#325CC0" "gray65") "red green blue magenta" ((0 114 #5=(font-lock-face (:foreground "red3"))) (2 100 #5#) (3 32 nil) (4 103 #6=(font-lock-face (:foreground "green3"))) (8 110 #6#) (9 32 nil) (10 98 #7=(font-lock-face (:foreground "blue2"))) (13 101 #7#) (14 32 nil) (15 109 #8=(font-lock-face (:foreground "magenta3"))) (20 116 #8#) (21 97 #8#))) (alabaster-themes-light-mono ("black" "#AA3731" "#000000" "#FFBC5D" "#000000" "#000000" "#000000" "gray65") "red green blue magenta" ((0 114 #9=(font-lock-face (:foreground "red3"))) (2 100 #9#) (3 32 nil) (4 103 #10=(font-lock-face (:foreground "green3"))) (8 110 #10#) (9 32 nil) (10 98 #11=(font-lock-face (:foreground "blue2"))) (13 101 #11#) (14 32 nil) (15 109 #12=(font-lock-face (:foreground "magenta3"))) (20 116 #12#) (21 97 #12#))) (alabaster-themes-dark ("black" "#DFDF8E" "#95CB82" "#CD974B" "#8AB1F0" "#CC8BC9" "#8AB1F0" "gray65") "red green blue magenta" ((0 114 #13=(font-lock-face (:foreground "red3"))) (2 100 #13#) (3 32 nil) (4 103 #14=(font-lock-face (:foreground "green3"))) (8 110 #14#) (9 32 nil) (10 98 #15=(font-lock-face (:foreground "blue2"))) (13 101 #15#) (14 32 nil) (15 109 #16=(font-lock-face (:foreground "magenta3"))) (20 116 #16#) (21 97 #16#))) (alabaster-themes-dark-mono ("black" "#ff6b6b" "#CECECE" "#CD974B" "#CECECE" "#CECECE" "#CECECE" "gray65") "red green blue magenta" ((0 114 #17=(font-lock-face (:foreground "red3"))) (2 100 #17#) (3 32 nil) (4 103 #18=(font-lock-face (:foreground "green3"))) (8 110 #18#) (9 32 nil) (10 98 #19=(font-lock-face (:foreground "blue2"))) (13 101 #19#) (14 32 nil) (15 109 #20=(font-lock-face (:foreground "magenta3"))) (20 116 #20#) (21 97 #20#))))"##
        ]],
    )
}

pub(super) fn rendering_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        real_emacs_lisp_font_lock_exercises_all_five_theme_highlighting_philosophies(),
        real_org_document_resolves_titles_todos_links_blocks_and_metadata_faces(),
        real_diff_buffer_applies_file_hunk_added_removed_context_and_refined_faces(),
        real_compilation_buffer_preserves_diagnostic_faces_and_location_metadata(),
        real_dired_listing_applies_directory_symlink_mark_and_flag_workflow_faces(),
        semantic_terminal_palette_drives_real_ansi_color_rendering_for_each_variant(),
    ]
}
