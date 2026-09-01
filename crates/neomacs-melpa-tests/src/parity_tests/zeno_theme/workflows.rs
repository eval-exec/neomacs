use expect_test::expect;

use super::ParityBatchCase;

fn loads_the_theme_into_a_real_elisp_release_review() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "zeno-elisp-review"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "release-review.el" root))
       (default-directory root)
       buffer result)
  (unwind-protect
      (save-window-excursion
        (neomacs-zeno-test--cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           ";;; release-review.el --- Verify a Unicode release\n\n"
           ";; Keep failed deployments visible to operators.\n\n"
           "(defconst release-review-limit 42\n"
           "  \"Maximum releases reviewed per batch.\")\n\n"
           "(defun release-ready-p (release)\n"
           "  \"Return non-nil when RELEASE Ω is ready.\"\n"
           "  (if (null release)\n"
           "      (error \"Missing release\")\n"
           "    (message \"reviewing %s\" release)\n"
           "    :ready))\n"))
        (setq buffer (find-file-noselect source))
        (switch-to-buffer buffer)
        (emacs-lisp-mode)
        (font-lock-ensure)
        (let ((before
               (list
                :themes (copy-sequence custom-enabled-themes)
                :default
                (list
                 (face-attribute 'default :foreground nil t)
                 (face-attribute 'default :background nil t)))))
          (neomacs-zeno-test--reload nil)
          (font-lock-flush)
          (font-lock-ensure)
          (goto-char (point-min))
          (search-forward "(message")
          (setq result
                (list
                 :before before
                 :file (file-relative-name buffer-file-name root)
                 :mode major-mode
                 :themes (copy-sequence custom-enabled-themes)
                 :point (point)
                 :line (line-number-at-pos)
                 :modified (buffer-modified-p)
                 :content
                 (buffer-substring-no-properties (point-min) (point-max))
                 :faces
                 (neomacs-zeno-test--face-state
                  '(default cursor fringe region highlight hl-line isearch
                    lazy-highlight mode-line mode-line-inactive
                    minibuffer-prompt line-number line-number-current-line))
                 :tokens
                 (neomacs-zeno-test--token-state
                  '(";;; release-review.el"
                    "Verify a Unicode release"
                    "Keep failed deployments"
                    "defconst"
                    "release-review-limit"
                    "42"
                    "Maximum releases"
                    "defun"
                    "release-ready-p"
                    "Return non-nil"
                    "if (null"
                    "null"
                    "error"
                    "\"Missing release\""
                    "message"
                    ":ready"))))))
    (neomacs-zeno-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r####"OK (:before (:themes nil :default ("unspecified-fg" "unspecified-bg")) :file "release-review.el" :mode emacs-lisp-mode :themes (zeno) :point 322 :line 12 :modified nil :content ";;; release-review.el --- Verify a Unicode release\n\n;; Keep failed deployments visible to operators.\n\n(defconst release-review-limit 42\n  \"Maximum releases reviewed per batch.\")\n\n(defun release-ready-p (release)\n  \"Return non-nil when RELEASE Ω is ready.\"\n  (if (null release)\n      (error \"Missing release\")\n    (message \"reviewing %s\" release)\n    :ready))\n" :faces ((default "#E8F0FF" "#282A36" "#E8F0FF" "#282A36" normal normal nil nil nil) (cursor unspecified "#F8F8F0" "#E8F0FF" "#F8F8F0" unspecified unspecified unspecified unspecified unspecified) (fringe unspecified "#282A36" "#E8F0FF" "#282A36" unspecified unspecified unspecified unspecified unspecified) (region unspecified "#665C7E" "#E8F0FF" "#665C7E" unspecified unspecified unspecified unspecified unspecified) (highlight "#000000" "#C1CAFF" "#000000" "#C1CAFF" unspecified unspecified unspecified unspecified unspecified) (hl-line :undefined) (isearch "#000000" "#C1CAFF" "#000000" "#C1CAFF" unspecified unspecified unspecified unspecified unspecified) (lazy-highlight "#000000" "#C1CAFF" "#000000" "#C1CAFF" unspecified unspecified unspecified unspecified unspecified) (mode-line "#E8F0FF" "#343850" "#E8F0FF" "#343850" unspecified unspecified unspecified (:line-width 1 :color "#1F2029" :style released-button) unspecified) (mode-line-inactive "#7a7a7a" "#1d2130" "#7a7a7a" "#1d2130" unspecified unspecified unspecified nil unspecified) (minibuffer-prompt "#66D9EF" unspecified "#66D9EF" "#282A36" unspecified unspecified unspecified unspecified unspecified) (line-number "#6883A8" "#282A36" "#6883A8" "#282A36" normal unspecified unspecified unspecified unspecified) (line-number-current-line "#BB98FC" "#282A36" "#BB98FC" "#282A36" normal unspecified unspecified unspecified unspecified)) :tokens ((";;; release-review.el" 1 font-lock-comment-delimiter-face nil "#6F7181" "#282A36" unspecified normal) ("Verify a Unicode release" 27 font-lock-comment-face nil "#6F7181" "#282A36" unspecified normal) ("Keep failed deployments" 56 font-lock-comment-face nil "#6F7181" "#282A36" unspecified normal) ("defconst" 104 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("release-review-limit" 113 font-lock-variable-name-face nil "#5FCA81" "#282A36" unspecified unspecified) ("42" 134 nil nil nil nil nil nil) ("Maximum releases" 140 font-lock-doc-face nil "#C1CAFF" "#282A36" unspecified normal) ("defun" 181 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("release-ready-p" 187 font-lock-function-name-face nil "#84B5FF" "#282A36" unspecified normal) ("Return non-nil" 216 font-lock-doc-face nil "#C1CAFF" "#282A36" unspecified normal) ("if (null" 260 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("null" 264 nil nil nil nil nil nil) ("error" 285 font-lock-warning-face nil "#FFFFFF" "#282A36" unspecified unspecified) ("\"Missing release\"" 291 font-lock-string-face nil "#C1CAFF" "#282A36" unspecified unspecified) ("message" 315 nil nil nil nil nil nil) (":ready" 351 font-lock-builtin-face nil "#BB98FC" "#282A36" unspecified unspecified)))"####
    ]];
    ParityBatchCase::value(
        "loads_the_theme_into_a_real_elisp_release_review",
        elisp_form,
        expect,
    )
}

fn documented_italics_option_restyles_live_source_only_after_reload() -> ParityBatchCase {
    let elisp_form = r####"
(let ((zeno-theme-enable-italics nil)
      plain reenabled italicized c-source result)
  (unwind-protect
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert
         ";; Explain why Ω is retained.\n"
         "(defun retain-release (release)\n"
         "  \"Keep RELEASE while operators investigate.\"\n"
         "  (when release\n"
         "    (message \"retaining %s\" release)))\n")
        (neomacs-zeno-test--reload nil)
        (font-lock-flush)
        (font-lock-ensure)
        (setq plain
              (list
               :option zeno-theme-enable-italics
               :tokens
               (neomacs-zeno-test--token-state
                '("Explain why Ω" "defun" "retain-release"
                  "Keep RELEASE" "when release" "\"retaining %s\""))
               :faces
               (neomacs-zeno-test--face-state
                '(font-lock-comment-face font-lock-comment-delimiter-face
                  font-lock-doc-face font-lock-function-name-face
                  font-lock-keyword-face))))
        (setq zeno-theme-enable-italics t)
        (disable-theme 'zeno)
        (enable-theme 'zeno)
        (font-lock-flush)
        (font-lock-ensure)
        (setq reenabled
              (list
               :option zeno-theme-enable-italics
               :tokens
               (neomacs-zeno-test--token-state
                '("Explain why Ω" "retain-release" "Keep RELEASE"))
               :faces
               (neomacs-zeno-test--face-state
                '(font-lock-comment-face font-lock-doc-face
                  font-lock-function-name-face))))
        (neomacs-zeno-test--reload t)
        (font-lock-flush)
        (font-lock-ensure)
        (setq italicized
              (list
               :option zeno-theme-enable-italics
               :text (buffer-substring-no-properties (point-min) (point-max))
               :modified (buffer-modified-p)
               :tokens
               (neomacs-zeno-test--token-state
                '("Explain why Ω" "defun" "retain-release"
                  "Keep RELEASE" "when release" "\"retaining %s\""))
               :faces
               (neomacs-zeno-test--face-state
                '(font-lock-comment-face font-lock-comment-delimiter-face
                  font-lock-doc-face font-lock-function-name-face
                  font-lock-keyword-face))))
        (setq c-source
              (with-temp-buffer
                (c-mode)
                (insert
                 "typedef struct release_record {\n"
                 "  int build_number;\n"
                 "  double confidence;\n"
                 "} release_record;\n\n"
                 "int publish_release(release_record *release) {\n"
                 "  return release->build_number;\n"
                 "}\n")
                (font-lock-ensure)
                (list
                 :mode major-mode
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :tokens
                 (neomacs-zeno-test--token-state
                  '("typedef" "struct" "release_record" "int build_number"
                    "double confidence" "publish_release" "return release"))
                 :type-face
                 (neomacs-zeno-test--face-state '(font-lock-type-face))
                 :recorded
                 (neomacs-zeno-test--theme-face-specs
                  '(font-lock-type-face)))))
        (setq result
              (list
               :themes (copy-sequence custom-enabled-themes)
               :plain plain
               :reenabled reenabled
               :italicized italicized
               :c-source c-source)))
    (neomacs-zeno-test--cleanup nil))
  result)
"####;
    let expect = expect![[
        r####"OK (:themes (zeno) :plain (:option nil :tokens (("Explain why Ω" 4 font-lock-comment-face nil "#6F7181" "#282A36" unspecified normal) ("defun" 32 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("retain-release" 38 font-lock-function-name-face nil "#84B5FF" "#282A36" unspecified normal) ("Keep RELEASE" 66 font-lock-doc-face nil "#C1CAFF" "#282A36" unspecified normal) ("when release" 112 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("\"retaining %s\"" 138 font-lock-string-face nil "#C1CAFF" "#282A36" unspecified unspecified)) :faces ((font-lock-comment-face "#6F7181" unspecified "#6F7181" "#282A36" unspecified normal unspecified unspecified unspecified) (font-lock-comment-delimiter-face "#6F7181" unspecified "#6F7181" "#282A36" unspecified normal unspecified unspecified unspecified) (font-lock-doc-face "#C1CAFF" unspecified "#C1CAFF" "#282A36" unspecified normal unspecified unspecified unspecified) (font-lock-function-name-face "#84B5FF" unspecified "#84B5FF" "#282A36" unspecified normal unspecified unspecified unspecified) (font-lock-keyword-face "#66D9EF" unspecified "#66D9EF" "#282A36" unspecified unspecified unspecified unspecified unspecified))) :reenabled (:option t :tokens (("Explain why Ω" 4 font-lock-comment-face nil "#6F7181" "#282A36" unspecified normal) ("retain-release" 38 font-lock-function-name-face nil "#84B5FF" "#282A36" unspecified normal) ("Keep RELEASE" 66 font-lock-doc-face nil "#C1CAFF" "#282A36" unspecified normal)) :faces ((font-lock-comment-face "#6F7181" unspecified "#6F7181" "#282A36" unspecified normal unspecified unspecified unspecified) (font-lock-doc-face "#C1CAFF" unspecified "#C1CAFF" "#282A36" unspecified normal unspecified unspecified unspecified) (font-lock-function-name-face "#84B5FF" unspecified "#84B5FF" "#282A36" unspecified normal unspecified unspecified unspecified))) :italicized (:option t :text ";; Explain why Ω is retained.\n(defun retain-release (release)\n  \"Keep RELEASE while operators investigate.\"\n  (when release\n    (message \"retaining %s\" release)))\n" :modified t :tokens (("Explain why Ω" 4 font-lock-comment-face nil "#6F7181" "#282A36" unspecified italic) ("defun" 32 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("retain-release" 38 font-lock-function-name-face nil "#84B5FF" "#282A36" unspecified italic) ("Keep RELEASE" 66 font-lock-doc-face nil "#C1CAFF" "#282A36" unspecified italic) ("when release" 112 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("\"retaining %s\"" 138 font-lock-string-face nil "#C1CAFF" "#282A36" unspecified unspecified)) :faces ((font-lock-comment-face "#6F7181" unspecified "#6F7181" "#282A36" unspecified italic unspecified unspecified unspecified) (font-lock-comment-delimiter-face "#6F7181" unspecified "#6F7181" "#282A36" unspecified italic unspecified unspecified unspecified) (font-lock-doc-face "#C1CAFF" unspecified "#C1CAFF" "#282A36" unspecified italic unspecified unspecified unspecified) (font-lock-function-name-face "#84B5FF" unspecified "#84B5FF" "#282A36" unspecified italic unspecified unspecified unspecified) (font-lock-keyword-face "#66D9EF" unspecified "#66D9EF" "#282A36" unspecified unspecified unspecified unspecified unspecified))) :c-source (:mode c-mode :text "typedef struct release_record {\n  int build_number;\n  double confidence;\n} release_record;\n\nint publish_release(release_record *release) {\n  return release->build_number;\n}\n" :tokens (("typedef" 1 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("struct" 9 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified) ("release_record" 16 font-lock-type-face nil "#66D9EF" "#282A36" unspecified unspecified) ("int build_number" 35 font-lock-type-face nil "#66D9EF" "#282A36" unspecified unspecified) ("double confidence" 55 font-lock-type-face nil "#66D9EF" "#282A36" unspecified unspecified) ("publish_release" 97 font-lock-function-name-face nil "#84B5FF" "#282A36" unspecified italic) ("return release" 142 font-lock-keyword-face nil "#66D9EF" "#282A36" unspecified unspecified)) :type-face ((font-lock-type-face "#66D9EF" unspecified "#66D9EF" "#282A36" unspecified unspecified unspecified unspecified unspecified)) :recorded ((font-lock-type-face ((t (:foreground "#66D9EF" : italic slant)))))))"####
    ]];
    ParityBatchCase::value(
        "documented_italics_option_restyles_live_source_only_after_reload",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn styles_real_diff_org_and_dired_review_workflows() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "zeno-built-in-workflows"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (default-directory root)
       diff-state org-state dired-state result)
  (unwind-protect
      (save-window-excursion
        (neomacs-zeno-test--cleanup root)
        (make-directory (expand-file-name "artifacts" root) t)
        (with-temp-file (expand-file-name "release.txt" root)
          (insert "release Ω\n"))
        (neomacs-zeno-test--reload nil)
        (with-temp-buffer
          (require 'diff-mode)
          (insert
           "diff --git a/release.txt b/release.txt\n"
           "index 1111111..2222222 100644\n"
           "--- a/release.txt\n"
           "+++ b/release.txt\n"
           "@@ -1,2 +1,2 @@\n"
           "-status=failed\n"
           "+status=ready Ω\n")
          (diff-mode)
          (font-lock-ensure)
          (setq diff-state
                (list
                 :mode major-mode
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :tokens
                 (neomacs-zeno-test--token-state
                  '("diff --git" "--- a/release.txt" "+++ b/release.txt"
                    "@@ -1,2 +1,2 @@" "-status=failed" "+status=ready Ω")))))
        (with-temp-buffer
          (require 'org)
          (insert
           "#+title: Release Ω Plan\n"
           "* TODO Ship candidate\n"
           "** DONE Verify artifacts\n"
           "*** Notes\n"
           "See [[file:release.txt][release record]].\n")
          (org-mode)
          (font-lock-ensure)
          (setq org-state
                (list
                 :mode major-mode
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :tokens
                 (neomacs-zeno-test--token-state
                  '("Release Ω Plan" "TODO" "Ship candidate" "DONE"
                    "Verify artifacts" "Notes" "release record")))))
        (let ((buffer (dired-noselect root)))
          (unwind-protect
              (with-current-buffer buffer
                (font-lock-ensure)
                (setq dired-state
                      (list
                       :mode major-mode
                       :directory (file-relative-name default-directory root)
                       :entries
                       (mapcar
                        (lambda (name)
                          (goto-char (point-min))
                          (search-forward name)
                          (list
                           name
                           (copy-tree
                            (get-char-property (match-beginning 0) 'face))))
                        '("artifacts" "release.txt"))
                       :directory-face
                       (neomacs-zeno-test--face-state '(dired-directory)))))
            (kill-buffer buffer)))
        (setq result
              (list
               :themes (copy-sequence custom-enabled-themes)
               :diff diff-state
               :org org-state
               :dired dired-state
               :faces
               (neomacs-zeno-test--face-state
                '(diff-header diff-hunk-header diff-added diff-removed
                  diff-refine-added diff-refine-removed
                  org-level-1 org-level-2 org-level-3 link dired-directory)))))
    (neomacs-zeno-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r####"OK (:themes (zeno) :diff (:mode diff-mode :text "diff --git a/release.txt b/release.txt\nindex 1111111..2222222 100644\n--- a/release.txt\n+++ b/release.txt\n@@ -1,2 +1,2 @@\n-status=failed\n+status=ready Ω\n" :tokens (("diff --git" 1 diff-header nil "#E8F0FF" "#232526" unspecified unspecified) ("--- a/release.txt" 70 diff-header nil "#E8F0FF" "#232526" unspecified unspecified) ("+++ b/release.txt" 88 diff-header nil "#E8F0FF" "#232526" unspecified unspecified) ("@@ -1,2 +1,2 @@" 106 diff-hunk-header nil "#A6E22E" "#232526" unspecified unspecified) ("-status=failed" 122 diff-indicator-removed nil "#D2527F" "#282A36" unspecified unspecified) ("+status=ready Ω" 137 diff-indicator-added nil "#A6E22E" "#282A36" unspecified unspecified))) :org (:mode org-mode :text "#+title: Release Ω Plan\n* TODO Ship candidate\n** DONE Verify artifacts\n*** Notes\nSee [[file:release.txt][release record]].\n" :tokens (("Release Ω Plan" 10 org-document-title nil "#E8F0FF" "#282A36" bold unspecified) ("TODO" 27 (org-todo org-level-1) nil "#E8F0FF" "#282A36" bold unspecified) ("Ship candidate" 32 org-level-1 nil "#84B5FF" "#282A36" unspecified unspecified) ("DONE" 50 (org-done org-level-2) nil "#E8F0FF" "#282A36" bold unspecified) ("Verify artifacts" 55 (org-headline-done org-level-2) nil "#E8F0FF" "#282A36" unspecified unspecified) ("Notes" 76 org-level-3 nil "#5FCA81" "#282A36" unspecified unspecified) ("release record" 106 org-link nil "#60FCEC" "#282A36" unspecified unspecified))) :dired (:mode dired-mode :directory "./" :entries (("artifacts" dired-directory) ("release.txt" nil)) :directory-face ((dired-directory "#84B5FF" unspecified "#84B5FF" "#282A36" unspecified unspecified unspecified unspecified unspecified))) :faces ((diff-header "#E8F0FF" "#232526" "#E8F0FF" "#232526" unspecified unspecified unspecified unspecified unspecified) (diff-hunk-header "#A6E22E" "#232526" "#A6E22E" "#232526" unspecified unspecified unspecified unspecified unspecified) (diff-added "#A6E22E" unspecified "#A6E22E" "#282A36" unspecified unspecified unspecified unspecified unspecified) (diff-removed "#D2527F" unspecified "#D2527F" "#282A36" unspecified unspecified unspecified unspecified unspecified) (diff-refine-added "#A6E22E" "#343850" "#A6E22E" "#343850" unspecified unspecified unspecified unspecified unspecified) (diff-refine-removed "#D2527F" "#343850" "#D2527F" "#343850" unspecified unspecified unspecified unspecified unspecified) (org-level-1 "#84B5FF" unspecified "#84B5FF" "#282A36" unspecified unspecified unspecified unspecified unspecified) (org-level-2 unspecified unspecified "#66D9EF" "#282A36" unspecified unspecified unspecified unspecified outline-1) (org-level-3 unspecified unspecified "#5FCA81" "#282A36" unspecified unspecified unspecified unspecified outline-3) (link "#60FCEC" "#282A36" "#60FCEC" "#282A36" unspecified unspecified t unspecified unspecified) (dired-directory "#84B5FF" unspecified "#84B5FF" "#282A36" unspecified unspecified unspecified unspecified unspecified)))"####
    ]];
    ParityBatchCase::value(
        "styles_real_diff_org_and_dired_review_workflows",
        elisp_form,
        expect,
    )
}

fn delayed_optional_face_tolerates_the_pinned_malformed_attributes() -> ParityBatchCase {
    let elisp_form = r####"
(let ((faces '(company-template-field font-lock-warning-face
               line-number-current-line)))
  (unwind-protect
      (progn
        (neomacs-zeno-test--reload nil)
        (let ((before
               (list
                :defined (and (facep 'company-template-field) t)
                :recorded (neomacs-zeno-test--theme-face-specs faces))))
          (defface company-template-field
            '((t (:foreground "sentinel-fg" :background "sentinel-bg")))
            "Synthetic Company template field used by the parity workflow.")
          (with-temp-buffer
            (insert "Deploy ${release-candidate} to production")
            (add-face-text-property
             8 28 'company-template-field nil)
            (list
             :themes (copy-sequence custom-enabled-themes)
             :before before
             :text (buffer-substring-no-properties (point-min) (point-max))
             :template
             (list
              :token (buffer-substring-no-properties 8 28)
              :face (get-char-property 8 'face))
             :after (neomacs-zeno-test--face-state faces)
             :recorded (neomacs-zeno-test--theme-face-specs faces)))))
    (neomacs-zeno-test--cleanup nil)))
"####;
    let expect = expect![[
        r####"OK (:themes (zeno) :before (:defined nil :recorded ((company-template-field ((t (:background: "#282A36" :foreground "#FFFFFF")))) (font-lock-warning-face ((t (:foreground "#FFFFFF" ':background "#333333")))) (line-number-current-line ((t :background "#282A36" :foreground "#BB98FC" :weight normal))))) :text "Deploy ${release-candidate} to production" :template (:token "${release-candidate}" :face company-template-field) :after ((company-template-field "#FFFFFF" unspecified "#FFFFFF" "#282A36" unspecified unspecified unspecified unspecified unspecified) (font-lock-warning-face "#FFFFFF" unspecified "#FFFFFF" "#282A36" unspecified unspecified unspecified unspecified unspecified) (line-number-current-line "#BB98FC" "#282A36" "#BB98FC" "#282A36" normal unspecified unspecified unspecified unspecified)) :recorded ((company-template-field ((t (:background: "#282A36" :foreground "#FFFFFF")))) (font-lock-warning-face ((t (:foreground "#FFFFFF" ':background "#333333")))) (line-number-current-line ((t :background "#282A36" :foreground "#BB98FC" :weight normal)))))"####
    ]];
    ParityBatchCase::value(
        "delayed_optional_face_tolerates_the_pinned_malformed_attributes",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn theme_lifecycle_restores_a_preexisting_user_theme() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (progn
      (deftheme neomacs-zeno-baseline "Synthetic user baseline.")
      (custom-theme-set-faces
       'neomacs-zeno-baseline
       '(default ((t (:foreground "#111111" :background "#eeeeee"))))
       '(font-lock-keyword-face ((t (:foreground "#aa0000" :weight bold)))))
      (enable-theme 'neomacs-zeno-baseline)
      (let ((baseline
             (list
              :themes (copy-sequence custom-enabled-themes)
              :faces
              (neomacs-zeno-test--face-state
               '(default font-lock-keyword-face)))))
        (neomacs-zeno-test--reload nil t)
        (let ((loaded-disabled
               (list
                :known (and (custom-theme-p 'zeno) t)
                :enabled (and (custom-theme-enabled-p 'zeno) t)
                :themes (copy-sequence custom-enabled-themes)
                :faces
                (neomacs-zeno-test--face-state
                 '(default font-lock-keyword-face)))))
          (enable-theme 'zeno)
          (let ((enabled
                 (list
                  :themes (copy-sequence custom-enabled-themes)
                  :faces
                  (neomacs-zeno-test--face-state
                   '(default font-lock-keyword-face)))))
            (disable-theme 'zeno)
            (let ((restored
                   (list
                    :themes (copy-sequence custom-enabled-themes)
                    :faces
                    (neomacs-zeno-test--face-state
                     '(default font-lock-keyword-face)))))
              (enable-theme 'zeno)
              (list
               :baseline baseline
               :loaded-disabled loaded-disabled
               :enabled enabled
               :restored restored
               :reenabled
               (list
                :themes (copy-sequence custom-enabled-themes)
                :faces
                (neomacs-zeno-test--face-state
                 '(default font-lock-keyword-face)))))))))
  (neomacs-zeno-test--cleanup nil))
"####;
    let expect = expect![[
        r####"OK (:baseline (:themes (neomacs-zeno-baseline) :faces ((default "#111111" "#eeeeee" "#111111" "#eeeeee" normal normal nil nil nil) (font-lock-keyword-face "#aa0000" unspecified "#aa0000" "#eeeeee" bold unspecified unspecified unspecified unspecified))) :loaded-disabled (:known t :enabled nil :themes (neomacs-zeno-baseline) :faces ((default "#111111" "#eeeeee" "#111111" "#eeeeee" normal normal nil nil nil) (font-lock-keyword-face "#aa0000" unspecified "#aa0000" "#eeeeee" bold unspecified unspecified unspecified unspecified))) :enabled (:themes (zeno neomacs-zeno-baseline) :faces ((default "#E8F0FF" "#282A36" "#E8F0FF" "#282A36" normal normal nil nil nil) (font-lock-keyword-face "#66D9EF" unspecified "#66D9EF" "#282A36" bold unspecified unspecified unspecified unspecified))) :restored (:themes (neomacs-zeno-baseline) :faces ((default "#111111" "#eeeeee" "#111111" "#eeeeee" normal normal nil nil nil) (font-lock-keyword-face "#aa0000" unspecified "#aa0000" "#eeeeee" bold unspecified unspecified unspecified unspecified))) :reenabled (:themes (zeno neomacs-zeno-baseline) :faces ((default "#E8F0FF" "#282A36" "#E8F0FF" "#282A36" normal normal nil nil nil) (font-lock-keyword-face "#66D9EF" unspecified "#66D9EF" "#282A36" bold unspecified unspecified unspecified unspecified))))"####
    ]];
    ParityBatchCase::value(
        "theme_lifecycle_restores_a_preexisting_user_theme",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loads_the_theme_into_a_real_elisp_release_review(),
        documented_italics_option_restyles_live_source_only_after_reload(),
        styles_real_diff_org_and_dired_review_workflows(),
        delayed_optional_face_tolerates_the_pinned_malformed_attributes(),
        theme_lifecycle_restores_a_preexisting_user_theme(),
    ]
}
