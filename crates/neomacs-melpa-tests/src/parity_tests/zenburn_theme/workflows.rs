use expect_test::expect;

use super::ParityBatchCase;

fn loads_the_theme_into_a_real_elisp_release_review() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "zenburn-elisp-review"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "release-review.el" root))
       (default-directory root)
       buffer result)
  (unwind-protect
      (save-window-excursion
        (neomacs-zenburn-test--cleanup root)
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
          (neomacs-zenburn-test--reload)
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
                 (neomacs-zenburn-test--face-state
                  '(default cursor fringe region isearch lazy-highlight
                    mode-line mode-line-inactive minibuffer-prompt))
                 :tokens
                 (neomacs-zenburn-test--token-state
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
    (neomacs-zenburn-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r####"OK (:before (:themes nil :default ("unspecified-fg" "unspecified-bg")) :file "release-review.el" :mode emacs-lisp-mode :themes (zenburn) :point 322 :line 12 :modified nil :content ";;; release-review.el --- Verify a Unicode release\n\n;; Keep failed deployments visible to operators.\n\n(defconst release-review-limit 42\n  \"Maximum releases reviewed per batch.\")\n\n(defun release-ready-p (release)\n  \"Return non-nil when RELEASE Ω is ready.\"\n  (if (null release)\n      (error \"Missing release\")\n    (message \"reviewing %s\" release)\n    :ready))\n" :faces ((default "#DCDCCC" "#3F3F3F" "#DCDCCC" "#3F3F3F" normal normal nil nil nil) (cursor "#DCDCCC" "#FFFFEF" "#DCDCCC" "#FFFFEF" unspecified unspecified unspecified unspecified unspecified) (fringe "#DCDCCC" "#4F4F4F" "#DCDCCC" "#4F4F4F" unspecified unspecified unspecified unspecified unspecified) (region unspecified "#2B2B2B" "#DCDCCC" "#2B2B2B" unspecified unspecified unspecified t unspecified) (isearch "#D0BF8F" "#5F5F5F" "#D0BF8F" "#5F5F5F" bold unspecified unspecified unspecified unspecified) (lazy-highlight "#D0BF8F" "#383838" "#D0BF8F" "#383838" bold unspecified unspecified unspecified unspecified) (mode-line "#8FB28F" "#2B2B2B" "#8FB28F" "#2B2B2B" unspecified unspecified unspecified unspecified unspecified) (mode-line-inactive "#5F7F5F" "#383838" "#5F7F5F" "#383838" unspecified unspecified unspecified unspecified unspecified) (minibuffer-prompt "#F0DFAF" unspecified "#F0DFAF" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified)) :tokens ((";;; release-review.el" 1 font-lock-comment-delimiter-face "#5F7F5F" "#3F3F3F" unspecified unspecified) ("Verify a Unicode release" 27 font-lock-comment-face "#7F9F7F" "#3F3F3F" unspecified unspecified) ("Keep failed deployments" 56 font-lock-comment-face "#7F9F7F" "#3F3F3F" unspecified unspecified) ("defconst" 104 font-lock-keyword-face "#F0DFAF" "#3F3F3F" bold unspecified) ("release-review-limit" 113 font-lock-variable-name-face "#DFAF8F" "#3F3F3F" unspecified unspecified) ("42" 134 nil nil nil nil nil) ("Maximum releases" 140 font-lock-doc-face "#9FC59F" "#3F3F3F" unspecified unspecified) ("defun" 181 font-lock-keyword-face "#F0DFAF" "#3F3F3F" bold unspecified) ("release-ready-p" 187 font-lock-function-name-face "#93E0E3" "#3F3F3F" unspecified unspecified) ("Return non-nil" 216 font-lock-doc-face "#9FC59F" "#3F3F3F" unspecified unspecified) ("if (null" 260 font-lock-keyword-face "#F0DFAF" "#3F3F3F" bold unspecified) ("null" 264 nil nil nil nil nil) ("error" 285 font-lock-warning-face "#D0BF8F" "#3F3F3F" bold unspecified) ("\"Missing release\"" 291 font-lock-string-face "#CC9393" "#3F3F3F" unspecified unspecified) ("message" 315 nil nil nil nil nil) (":ready" 351 font-lock-builtin-face "#DCDCCC" "#3F3F3F" bold unspecified)))"####
    ]];
    ParityBatchCase::value(
        "loads_the_theme_into_a_real_elisp_release_review",
        elisp_form,
        expect,
    )
}

fn custom_palette_layers_repaint_real_source_and_diff_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(let (source-state diff-state result)
  (unwind-protect
      (progn
        (require 'ansi-color)
        (require 'vc-annotate)
        (neomacs-zenburn-test--reload
         :colors
         '(("zenburn-bg" . "#102030")
           ("zenburn-red" . "#ef4567")
           ("zenburn-blue" . "#2468ac"))
         :semantic-colors
         '(("zenburn-comment" . zenburn-blue)
           ("zenburn-error" . "#ff0055")
           ("zenburn-diff-added-bg" . "#113322")
           ("zenburn-diff-added-fg" . zenburn-cyan)))
        (with-temp-buffer
          (emacs-lisp-mode)
          (insert
           ";; Release Ω failed\n"
           "(defun deploy-release (name)\n"
           "  (if name\n"
           "      (message \"deploy %s\" name)\n"
           "    (error \"missing\")))\n")
          (font-lock-ensure)
          (setq source-state
                (list
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :tokens
                 (neomacs-zenburn-test--token-state
                  '("Release Ω failed" "defun" "deploy-release"
                    "if name" "\"deploy %s\"" "error" "\"missing\"")))))
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
                 (neomacs-zenburn-test--token-state
                  '("diff --git" "--- a/release.txt" "+++ b/release.txt"
                    "@@ -1,2 +1,2 @@" "-status=failed" "+status=ready Ω")))))
        (setq result
              (list
               :themes (copy-sequence custom-enabled-themes)
               :source source-state
               :diff diff-state
               :faces
               (neomacs-zenburn-test--face-state
                '(default region error font-lock-comment-face
                  font-lock-string-face diff-added diff-removed
                  diff-refine-added diff-refine-removed))
               :specs
               (neomacs-zenburn-test--theme-face-specs
                '(default error font-lock-comment-face font-lock-string-face
                  diff-added diff-removed diff-refine-added))
               :variables
               (list
                :ansi (append ansi-color-names-vector nil)
                :vc-background vc-annotate-background
                :defaults-unchanged
                (list
                 (cdr (assoc "zenburn-bg" zenburn-default-colors-alist))
                 (cdr (assoc "zenburn-red" zenburn-default-colors-alist))
                 (cdr (assoc "zenburn-comment"
                             zenburn-default-semantic-colors-alist)))))))
    (neomacs-zenburn-test--cleanup nil))
  result)
"####;
    let expect = expect![[
        r####"OK (:themes (zenburn) :source (:text ";; Release Ω failed\n(defun deploy-release (name)\n  (if name\n      (message \"deploy %s\" name)\n    (error \"missing\")))\n" :tokens (("Release Ω failed" 4 font-lock-comment-face "#2468ac" "#102030" unspecified unspecified) ("defun" 22 font-lock-keyword-face "#F0DFAF" "#102030" bold unspecified) ("deploy-release" 28 font-lock-function-name-face "#93E0E3" "#102030" unspecified unspecified) ("if name" 53 font-lock-keyword-face "#F0DFAF" "#102030" bold unspecified) ("\"deploy %s\"" 76 font-lock-string-face "#ef4567" "#102030" unspecified unspecified) ("error" 99 font-lock-warning-face "#D0BF8F" "#102030" bold unspecified) ("\"missing\"" 105 font-lock-string-face "#ef4567" "#102030" unspecified unspecified))) :diff (:mode diff-mode :text "diff --git a/release.txt b/release.txt\nindex 1111111..2222222 100644\n--- a/release.txt\n+++ b/release.txt\n@@ -1,2 +1,2 @@\n-status=failed\n+status=ready Ω\n" :tokens (("diff --git" 1 diff-header "#DCDCCC" "#5F5F5F" unspecified unspecified) ("--- a/release.txt" 70 diff-header "#DCDCCC" "#5F5F5F" unspecified unspecified) ("+++ b/release.txt" 88 diff-header "#DCDCCC" "#5F5F5F" unspecified unspecified) ("@@ -1,2 +1,2 @@" 106 diff-hunk-header "#DCDCCC" "#5F5F5F" unspecified unspecified) ("-status=failed" 122 diff-indicator-removed "#AC7373" "#553333" unspecified unspecified) ("+status=ready Ω" 137 diff-indicator-added "#93E0E3" "#113322" unspecified unspecified))) :faces ((default "#DCDCCC" "#102030" "#DCDCCC" "#102030" normal normal nil nil nil) (region unspecified "#2B2B2B" "#DCDCCC" "#2B2B2B" unspecified unspecified unspecified t unspecified) (error "#ff0055" unspecified "#ff0055" "#102030" bold unspecified unspecified unspecified unspecified) (font-lock-comment-face "#2468ac" unspecified "#2468ac" "#102030" unspecified unspecified unspecified unspecified unspecified) (font-lock-string-face "#ef4567" unspecified "#ef4567" "#102030" unspecified unspecified unspecified unspecified unspecified) (diff-added "#93E0E3" "#113322" "#93E0E3" "#113322" unspecified unspecified unspecified unspecified unspecified) (diff-removed "#AC7373" "#553333" "#AC7373" "#553333" unspecified unspecified unspecified unspecified unspecified) (diff-refine-added "#BFEBBF" "#338833" "#BFEBBF" "#338833" unspecified unspecified unspecified unspecified unspecified) (diff-refine-removed "#ef4567" "#883333" "#ef4567" "#883333" unspecified unspecified unspecified unspecified unspecified)) :specs ((default ((t (:foreground "#DCDCCC" :background "#102030")))) (error ((t (:foreground "#ff0055" :weight bold)))) (font-lock-comment-face ((t (:foreground "#2468ac")))) (font-lock-string-face ((t (:foreground "#ef4567")))) (diff-added ((t (:background "#113322" :foreground "#93E0E3")))) (diff-removed ((t (:background "#553333" :foreground "#AC7373")))) (diff-refine-added ((t (:background "#338833" :foreground "#BFEBBF"))))) :variables (:ansi ("#102030" "#ef4567" "#7F9F7F" "#F0DFAF" "#2468ac" "#DC8CC3" "#93E0E3" "#DCDCCC") :vc-background "#2B2B2B" :defaults-unchanged ("#3F3F3F" "#CC9393" zenburn-green)))"####
    ]];
    ParityBatchCase::value(
        "custom_palette_layers_repaint_real_source_and_diff_buffers",
        elisp_form,
        expect,
    )
}

fn scaled_variable_pitch_headings_survive_a_live_org_reload() -> ParityBatchCase {
    let elisp_form = r####"
(let (scaled scaled-outline plain plain-outline result)
  (unwind-protect
      (with-temp-buffer
        (require 'org)
        (insert
         "#+title: Release Ω Plan\n"
         "* TODO Ship candidate\n"
         "** DONE Verify artifacts\n"
         "*** Notes\n"
         "**** Risks\n"
         "***** Archive\n"
         "Paragraph with [[https://example.invalid][runbook]].\n")
        (org-mode)
        (neomacs-zenburn-test--reload
         :variable-pitch t
         :scale-org t
         :scale-outline t
         :height-1 1.11
         :height-2 1.22
         :height-3 1.33
         :height-4 1.44)
        (font-lock-flush)
        (font-lock-ensure)
        (setq scaled
              (list
               :text (buffer-substring-no-properties (point-min) (point-max))
               :tokens
               (neomacs-zenburn-test--token-state
                '("Release Ω Plan" "Ship candidate" "Verify artifacts"
                  "Notes" "Risks" "Archive" "runbook"))
               :specs
               (neomacs-zenburn-test--theme-face-specs
                '(zenburn-variable-pitch org-document-title
                  org-level-1 org-level-2 org-level-3 org-level-4 org-level-5
                  outline-1 outline-2 outline-3 outline-4 outline-5))))
        (setq scaled-outline
              (with-temp-buffer
                (insert
                 "* Release process\n"
                 "** Build candidate\n"
                 "*** Verify artifacts\n"
                 "**** Publish Ω\n"
                 "***** Archive\n")
                (outline-mode)
                (font-lock-ensure)
                (list
                 :mode major-mode
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :tokens
                 (neomacs-zenburn-test--token-state
                  '("Release process" "Build candidate" "Verify artifacts"
                    "Publish Ω" "Archive")))))
        (neomacs-zenburn-test--reload)
        (font-lock-flush)
        (font-lock-ensure)
        (setq plain
              (list
               :tokens
               (neomacs-zenburn-test--token-state
                '("Release Ω Plan" "Ship candidate" "Verify artifacts"
                  "Notes" "Risks" "Archive" "runbook"))
               :specs
               (neomacs-zenburn-test--theme-face-specs
                '(zenburn-variable-pitch org-document-title
                  org-level-1 org-level-2 org-level-3 org-level-4 org-level-5
                  outline-1 outline-2 outline-3 outline-4 outline-5))))
        (setq plain-outline
              (with-temp-buffer
                (insert
                 "* Release process\n"
                 "** Build candidate\n"
                 "*** Verify artifacts\n"
                 "**** Publish Ω\n"
                 "***** Archive\n")
                (outline-mode)
                (font-lock-ensure)
                (list
                 :mode major-mode
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :tokens
                 (neomacs-zenburn-test--token-state
                  '("Release process" "Build candidate" "Verify artifacts"
                    "Publish Ω" "Archive")))))
        (setq result
              (list
               :mode major-mode
               :themes (copy-sequence custom-enabled-themes)
               :scaled scaled
               :scaled-outline scaled-outline
               :plain plain
               :plain-outline plain-outline)))
    (neomacs-zenburn-test--cleanup nil))
  result)
"####;
    let expect = expect![[
        r####"OK (:mode org-mode :themes (zenburn) :scaled (:text "#+title: Release Ω Plan\n* TODO Ship candidate\n** DONE Verify artifacts\n*** Notes\n**** Risks\n***** Archive\nParagraph with [[https://example.invalid][runbook]].\n" :tokens (("Release Ω Plan" 10 org-document-title "#8CD0D3" "#3F3F3F" bold unspecified) ("Ship candidate" 32 org-level-1 "#DFAF8F" "#3F3F3F" unspecified unspecified) ("Verify artifacts" 55 (org-headline-done org-level-2) "#AFD8AF" "#3F3F3F" unspecified unspecified) ("Notes" 76 org-level-3 "#7CB8BB" "#3F3F3F" unspecified unspecified) ("Risks" 87 org-level-4 "#D0BF8F" "#3F3F3F" unspecified unspecified) ("Archive" 99 org-level-5 "#93E0E3" "#3F3F3F" unspecified unspecified) ("runbook" 149 org-link "#D0BF8F" "#3F3F3F" unspecified unspecified)) :specs ((zenburn-variable-pitch ((t (:inherit variable-pitch)))) (org-document-title ((t (:inherit zenburn-variable-pitch :foreground "#8CD0D3" :weight bold :height 1.44)))) (org-level-1 ((t (:inherit zenburn-variable-pitch :foreground "#DFAF8F" :height 1.44)))) (org-level-2 ((t (:inherit zenburn-variable-pitch :foreground "#BFEBBF" :height 1.33)))) (org-level-3 ((t (:inherit zenburn-variable-pitch :foreground "#7CB8BB" :height 1.22)))) (org-level-4 ((t (:inherit zenburn-variable-pitch :foreground "#D0BF8F" :height 1.11)))) (org-level-5 ((t (:inherit zenburn-variable-pitch :foreground "#93E0E3")))) (outline-1 ((t (:inherit zenburn-variable-pitch :foreground "#DFAF8F" :height 1.44)))) (outline-2 ((t (:inherit zenburn-variable-pitch :foreground "#BFEBBF" :height 1.33)))) (outline-3 ((t (:inherit zenburn-variable-pitch :foreground "#7CB8BB" :height 1.22)))) (outline-4 ((t (:inherit zenburn-variable-pitch :foreground "#D0BF8F" :height 1.11)))) (outline-5 ((t (:inherit zenburn-variable-pitch :foreground "#93E0E3")))))) :scaled-outline (:mode outline-mode :text "* Release process\n** Build candidate\n*** Verify artifacts\n**** Publish Ω\n***** Archive\n" :tokens (("Release process" 3 outline-1 "#DFAF8F" "#3F3F3F" unspecified unspecified) ("Build candidate" 22 outline-2 "#BFEBBF" "#3F3F3F" unspecified unspecified) ("Verify artifacts" 42 outline-3 "#7CB8BB" "#3F3F3F" unspecified unspecified) ("Publish Ω" 64 outline-4 "#D0BF8F" "#3F3F3F" unspecified unspecified) ("Archive" 80 outline-5 "#93E0E3" "#3F3F3F" unspecified unspecified))) :plain (:tokens (("Release Ω Plan" 10 org-document-title "#8CD0D3" "#3F3F3F" bold normal) ("Ship candidate" 32 org-level-1 "#DFAF8F" "#3F3F3F" normal normal) ("Verify artifacts" 55 (org-headline-done org-level-2) "#AFD8AF" "#3F3F3F" unspecified unspecified) ("Notes" 76 org-level-3 "#7CB8BB" "#3F3F3F" normal normal) ("Risks" 87 org-level-4 "#D0BF8F" "#3F3F3F" normal normal) ("Archive" 99 org-level-5 "#93E0E3" "#3F3F3F" normal normal) ("runbook" 149 org-link "#D0BF8F" "#3F3F3F" unspecified unspecified)) :specs ((zenburn-variable-pitch ((t (:inherit default)))) (org-document-title ((t (:inherit zenburn-variable-pitch :foreground "#8CD0D3" :weight bold)))) (org-level-1 ((t (:inherit zenburn-variable-pitch :foreground "#DFAF8F")))) (org-level-2 ((t (:inherit zenburn-variable-pitch :foreground "#BFEBBF")))) (org-level-3 ((t (:inherit zenburn-variable-pitch :foreground "#7CB8BB")))) (org-level-4 ((t (:inherit zenburn-variable-pitch :foreground "#D0BF8F")))) (org-level-5 ((t (:inherit zenburn-variable-pitch :foreground "#93E0E3")))) (outline-1 ((t (:inherit zenburn-variable-pitch :foreground "#DFAF8F")))) (outline-2 ((t (:inherit zenburn-variable-pitch :foreground "#BFEBBF")))) (outline-3 ((t (:inherit zenburn-variable-pitch :foreground "#7CB8BB")))) (outline-4 ((t (:inherit zenburn-variable-pitch :foreground "#D0BF8F")))) (outline-5 ((t (:inherit zenburn-variable-pitch :foreground "#93E0E3")))))) :plain-outline (:mode outline-mode :text "* Release process\n** Build candidate\n*** Verify artifacts\n**** Publish Ω\n***** Archive\n" :tokens (("Release process" 3 outline-1 "#DFAF8F" "#3F3F3F" normal normal) ("Build candidate" 22 outline-2 "#BFEBBF" "#3F3F3F" normal normal) ("Verify artifacts" 42 outline-3 "#7CB8BB" "#3F3F3F" normal normal) ("Publish Ω" 64 outline-4 "#D0BF8F" "#3F3F3F" normal normal) ("Archive" 80 outline-5 "#93E0E3" "#3F3F3F" normal normal))))"####
    ]];
    ParityBatchCase::value(
        "scaled_variable_pitch_headings_survive_a_live_org_reload",
        elisp_form,
        expect,
    )
}

fn theme_variables_colorize_a_real_terminal_build_log() -> ParityBatchCase {
    let elisp_form = r####"
(let (result)
  (unwind-protect
      (progn
        (require 'ansi-color)
        (require 'vc-annotate)
        (neomacs-zenburn-test--reload)
        (with-temp-buffer
          (insert
           (ansi-color-apply
            (concat
             "deploy \e[31mFAILED\e[0m, "
             "\e[32m42 passed\e[0m; "
             "\e[1;34mretry\e[0m\n")))
          (setq result
                (list
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :font-lock-runs
                 (neomacs-zenburn-test--property-runs 'font-lock-face)
                 :faces
                 (neomacs-zenburn-test--face-state
                  '(ansi-color-red ansi-color-green ansi-color-blue
                    ansi-color-bold))
                 :ansi-vector (append ansi-color-names-vector nil)
                 :vc
                 (list
                  :background vc-annotate-background
                  :very-old vc-annotate-very-old-color
                  :map (copy-tree vc-annotate-color-map))
                 :themes (copy-sequence custom-enabled-themes)))))
    (neomacs-zenburn-test--cleanup nil))
  result)
"####;
    let expect = expect![[
        r####"OK (:text "deploy FAILED, 42 passed; retry\n" :font-lock-runs (("FAILED" (:foreground "#AC7373") 8 14) ("42 passed" (:foreground "#7F9F7F") 16 25) ("retry" (ansi-color-bold (:foreground "#7CB8BB")) 27 32)) :faces ((ansi-color-red "#AC7373" "#8C5353" "#AC7373" "#8C5353" unspecified unspecified unspecified unspecified unspecified) (ansi-color-green "#7F9F7F" "#9FC59F" "#7F9F7F" "#9FC59F" unspecified unspecified unspecified unspecified unspecified) (ansi-color-blue "#7CB8BB" "#4C7073" "#7CB8BB" "#4C7073" unspecified unspecified unspecified unspecified unspecified) (ansi-color-bold unspecified unspecified "#DCDCCC" "#3F3F3F" bold unspecified unspecified unspecified bold)) :ansi-vector ("#3F3F3F" "#CC9393" "#7F9F7F" "#F0DFAF" "#8CD0D3" "#DC8CC3" "#93E0E3" "#DCDCCC") :vc (:background "#2B2B2B" :very-old "#DC8CC3" :map ((20 . "#BC8383") (40 . "#CC9393") (60 . "#DFAF8F") (80 . "#D0BF8F") (100 . "#E0CF9F") (120 . "#F0DFAF") (140 . "#5F7F5F") (160 . "#7F9F7F") (180 . "#8FB28F") (200 . "#9FC59F") (220 . "#AFD8AF") (240 . "#BFEBBF") (260 . "#93E0E3") (280 . "#6CA0A3") (300 . "#7CB8BB") (320 . "#8CD0D3") (340 . "#94BFF3") (360 . "#DC8CC3"))) :themes (zenburn))"####
    ]];
    ParityBatchCase::value(
        "theme_variables_colorize_a_real_terminal_build_log",
        elisp_form,
        expect,
    )
}

fn rainbow_mode_toggles_zenburn_palette_names_in_real_lisp_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(let ((original-setting zenburn-add-font-lock-keywords)
      (original-cache zenburn-colors-font-lock-keywords)
      result)
  (unwind-protect
      (progn
        (setq zenburn-colors-font-lock-keywords nil)
        (cl-labels
            ((token-faces
              ()
              (mapcar
               (lambda (token)
                 (goto-char (point-min))
                 (search-forward token)
                 (list token
                       (copy-tree
                        (get-text-property (match-beginning 0) 'face))))
               '("zenburn-bg" "zenburn-red" "#123456")))
             (exercise
              (enabled mode file)
              (with-temp-buffer
                (funcall mode)
                (setq buffer-file-name file)
                (insert
                 "(list zenburn-bg zenburn-red \"#123456\") ; café palette\n")
                (let ((zenburn-add-font-lock-keywords enabled))
                  (rainbow-mode 1)
                  (font-lock-ensure)
                  (let ((on (token-faces)))
                    (rainbow-mode -1)
                    (font-lock-flush)
                    (font-lock-ensure)
                    (list
                     :setting enabled
                     :mode major-mode
                     :file (and file (file-name-nondirectory file))
                     :on on
                     :off (token-faces)
                     :minor-mode rainbow-mode))))))
          (setq result
                (list
                 :advice
                 (list
                  (and
                   (advice-member-p
                    #'zenburn--rainbow-turn-on 'rainbow-turn-on)
                   t)
                  (and
                   (advice-member-p
                    #'zenburn--rainbow-turn-off 'rainbow-turn-off)
                   t))
                 :configured
                 (exercise t 'emacs-lisp-mode
                           "/workspace/releases/palette-review.el")
                 :disabled
                 (exercise nil 'emacs-lisp-mode
                           "/workspace/releases/palette-review.el")
                 :theme-source
                 (exercise nil 'emacs-lisp-mode
                           "/workspace/themes/zenburn-theme.el")
                 :wrong-mode
                 (exercise t 'text-mode
                           "/workspace/releases/palette-review.txt")
                 :keyword-cache
                 (list
                  :entries (length zenburn-colors-font-lock-keywords)
                  :matches-bg
                  (and
                   (string-match-p
                    (caar zenburn-colors-font-lock-keywords) "zenburn-bg")
                   t)
                  :matches-red
                  (and
                   (string-match-p
                    (caar zenburn-colors-font-lock-keywords) "zenburn-red")
                   t))))))
    (setq zenburn-add-font-lock-keywords original-setting
          zenburn-colors-font-lock-keywords original-cache))
  result)
"####;
    let expect = expect![[
        r##"OK (:advice (t t) :configured (:setting t :mode emacs-lisp-mode :file "palette-review.el" :on (("zenburn-bg" ((:foreground "white") (:background "#3F3F3F"))) ("zenburn-red" ((:foreground "black") (:background "#CC9393"))) ("#123456" ((:foreground "white") (:background "#123456")))) :off (("zenburn-bg" nil) ("zenburn-red" nil) ("#123456" font-lock-string-face)) :minor-mode nil) :disabled (:setting nil :mode emacs-lisp-mode :file "palette-review.el" :on (("zenburn-bg" nil) ("zenburn-red" nil) ("#123456" ((:foreground "white") (:background "#123456")))) :off (("zenburn-bg" nil) ("zenburn-red" nil) ("#123456" font-lock-string-face)) :minor-mode nil) :theme-source (:setting nil :mode emacs-lisp-mode :file "zenburn-theme.el" :on (("zenburn-bg" ((:foreground "white") (:background "#3F3F3F"))) ("zenburn-red" ((:foreground "black") (:background "#CC9393"))) ("#123456" ((:foreground "white") (:background "#123456")))) :off (("zenburn-bg" nil) ("zenburn-red" nil) ("#123456" font-lock-string-face)) :minor-mode nil) :wrong-mode (:setting t :mode text-mode :file "palette-review.txt" :on (("zenburn-bg" nil) ("zenburn-red" nil) ("#123456" ((:foreground "white") (:background "#123456")))) :off (("zenburn-bg" nil) ("zenburn-red" nil) ("#123456" nil)) :minor-mode nil) :keyword-cache (:entries 1 :matches-bg t :matches-red t))"##
    ]];
    ParityBatchCase::value(
        "rainbow_mode_toggles_zenburn_palette_names_in_real_lisp_buffers",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn late_defined_package_faces_receive_the_enabled_theme_in_real_content() -> ParityBatchCase {
    let elisp_form = r####"
(let ((faces
       '(asciidoc-document-title-face
         cider-repl-stderr-face
         inf-ruby-result-overlay-face
         vundo-diff-highlight
         easy-kill-selection
         copilot-overlay-face
         mistty-fringe-face
         keycast-key
         dictionary-reference-face
         clojure-keyword-face
         haskell-error-face
         erlang-edoc-heading
         git-timemachine-commit
         gptel-context-deletion-face
         magit-diff-added))
      before enabled rendered disabled reenabled result)
  (unwind-protect
      (progn
        (neomacs-zenburn-test--cleanup nil)
        (neomacs-zenburn-test--reload)
        (setq before
              (mapcar (lambda (face) (list face (and (facep face) t))) faces))
        (dolist (face faces)
          (eval
           `(defface ,face
              '((t (:foreground "#ff00ff"
                    :background "#00ffff"
                    :weight ultra-bold
                    :slant italic
                    :underline t)))
              ,(format "Late package face fixture for %s." face))))
        (setq enabled (neomacs-zenburn-test--face-state faces))
        (with-temp-buffer
          (dolist (face faces)
            (let ((start (point)))
              (insert (symbol-name face) "\n")
              (add-text-properties start (1- (point)) `(face ,face))))
          (setq rendered
                (list
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :runs (neomacs-zenburn-test--property-runs 'face))))
        (disable-theme 'zenburn)
        (setq disabled (neomacs-zenburn-test--face-state faces))
        (enable-theme 'zenburn)
        (setq reenabled (neomacs-zenburn-test--face-state faces))
        (setq result
              (list
               :absent-before before
               :enabled enabled
               :rendered rendered
               :disabled disabled
               :reenabled reenabled
               :round-trip (equal enabled reenabled)
               :themes (copy-sequence custom-enabled-themes))))
    (neomacs-zenburn-test--cleanup nil))
  result)
"####;
    let expect = expect![[
        r##"OK (:absent-before ((asciidoc-document-title-face nil) (cider-repl-stderr-face nil) (inf-ruby-result-overlay-face nil) (vundo-diff-highlight nil) (easy-kill-selection nil) (copilot-overlay-face nil) (mistty-fringe-face nil) (keycast-key nil) (dictionary-reference-face nil) (clojure-keyword-face nil) (haskell-error-face nil) (erlang-edoc-heading nil) (git-timemachine-commit nil) (gptel-context-deletion-face nil) (magit-diff-added nil)) :enabled ((asciidoc-document-title-face unspecified unspecified "#DCDCCC" "#3F3F3F" unspecified unspecified unspecified unspecified adoc-title-0-face) (cider-repl-stderr-face "#CC9393" unspecified "#CC9393" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified) (inf-ruby-result-overlay-face unspecified unspecified "#DCDCCC" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified) (vundo-diff-highlight "#DFAF8F" unspecified "#DFAF8F" "#3F3F3F" bold unspecified unspecified unspecified unspecified) (easy-kill-selection unspecified "#2B2B2B" "#DCDCCC" "#2B2B2B" unspecified unspecified unspecified t unspecified) (copilot-overlay-face "#656555" unspecified "#656555" "#3F3F3F" unspecified italic unspecified unspecified unspecified) (mistty-fringe-face "#5F5F5F" unspecified "#5F5F5F" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified) (keycast-key "#3F3F3F" "#8CD0D3" "#3F3F3F" "#8CD0D3" bold unspecified unspecified unspecified unspecified) (dictionary-reference-face unspecified unspecified "#F0DFAF" "#3F3F3F" bold unspecified t unspecified link) (clojure-keyword-face "#93E0E3" unspecified "#93E0E3" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified) (haskell-error-face "#BC8383" unspecified "#BC8383" "#3F3F3F" bold unspecified t unspecified unspecified) (erlang-edoc-heading "#DFAF8F" unspecified "#DFAF8F" "#3F3F3F" bold unspecified unspecified unspecified unspecified) (git-timemachine-commit "#DFAF8F" unspecified "#DFAF8F" "#3F3F3F" bold unspecified unspecified unspecified unspecified) (gptel-context-deletion-face unspecified "#553333" "#DCDCCC" "#553333" unspecified unspecified unspecified t unspecified) (magit-diff-added unspecified "#5F7F5F" "#DCDCCC" "#5F7F5F" unspecified unspecified unspecified unspecified unspecified)) :rendered (:text "asciidoc-document-title-face\ncider-repl-stderr-face\ninf-ruby-result-overlay-face\nvundo-diff-highlight\neasy-kill-selection\ncopilot-overlay-face\nmistty-fringe-face\nkeycast-key\ndictionary-reference-face\nclojure-keyword-face\nhaskell-error-face\nerlang-edoc-heading\ngit-timemachine-commit\ngptel-context-deletion-face\nmagit-diff-added\n" :runs (("asciidoc-document-title-face" asciidoc-document-title-face 1 29) ("cider-repl-stderr-face" cider-repl-stderr-face 30 52) ("inf-ruby-result-overlay-face" inf-ruby-result-overlay-face 53 81) ("vundo-diff-highlight" vundo-diff-highlight 82 102) ("easy-kill-selection" easy-kill-selection 103 122) ("copilot-overlay-face" copilot-overlay-face 123 143) ("mistty-fringe-face" mistty-fringe-face 144 162) ("keycast-key" keycast-key 163 174) ("dictionary-reference-face" dictionary-reference-face 175 200) ("clojure-keyword-face" clojure-keyword-face 201 221) ("haskell-error-face" haskell-error-face 222 240) ("erlang-edoc-heading" erlang-edoc-heading 241 260) ("git-timemachine-commit" git-timemachine-commit 261 283) ("gptel-context-deletion-face" gptel-context-deletion-face 284 311) ("magit-diff-added" magit-diff-added 312 328))) :disabled ((asciidoc-document-title-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (cider-repl-stderr-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (inf-ruby-result-overlay-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (vundo-diff-highlight "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (easy-kill-selection "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (copilot-overlay-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (mistty-fringe-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (keycast-key "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (dictionary-reference-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (clojure-keyword-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (haskell-error-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (erlang-edoc-heading "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (git-timemachine-commit "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (gptel-context-deletion-face "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified) (magit-diff-added "#ff00ff" "#00ffff" "#ff00ff" "#00ffff" ultra-bold italic t unspecified unspecified)) :reenabled ((asciidoc-document-title-face unspecified unspecified "#DCDCCC" "#3F3F3F" unspecified unspecified unspecified unspecified adoc-title-0-face) (cider-repl-stderr-face "#CC9393" unspecified "#CC9393" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified) (inf-ruby-result-overlay-face unspecified unspecified "#DCDCCC" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified) (vundo-diff-highlight "#DFAF8F" unspecified "#DFAF8F" "#3F3F3F" bold unspecified unspecified unspecified unspecified) (easy-kill-selection unspecified "#2B2B2B" "#DCDCCC" "#2B2B2B" unspecified unspecified unspecified t unspecified) (copilot-overlay-face "#656555" unspecified "#656555" "#3F3F3F" unspecified italic unspecified unspecified unspecified) (mistty-fringe-face "#5F5F5F" unspecified "#5F5F5F" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified) (keycast-key "#3F3F3F" "#8CD0D3" "#3F3F3F" "#8CD0D3" bold unspecified unspecified unspecified unspecified) (dictionary-reference-face unspecified unspecified "#F0DFAF" "#3F3F3F" bold unspecified t unspecified link) (clojure-keyword-face "#93E0E3" unspecified "#93E0E3" "#3F3F3F" unspecified unspecified unspecified unspecified unspecified) (haskell-error-face "#BC8383" unspecified "#BC8383" "#3F3F3F" bold unspecified t unspecified unspecified) (erlang-edoc-heading "#DFAF8F" unspecified "#DFAF8F" "#3F3F3F" bold unspecified unspecified unspecified unspecified) (git-timemachine-commit "#DFAF8F" unspecified "#DFAF8F" "#3F3F3F" bold unspecified unspecified unspecified unspecified) (gptel-context-deletion-face unspecified "#553333" "#DCDCCC" "#553333" unspecified unspecified unspecified t unspecified) (magit-diff-added unspecified "#5F7F5F" "#DCDCCC" "#5F7F5F" unspecified unspecified unspecified unspecified unspecified)) :round-trip t :themes (zenburn))"##
    ]];
    ParityBatchCase::value(
        "late_defined_package_faces_receive_the_enabled_theme_in_real_content",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn disabling_zenburn_restores_a_preexisting_theme_and_variables() -> ParityBatchCase {
    let elisp_form = r####"
(let ((faces
       '(default cursor region isearch font-lock-comment-face
         font-lock-keyword-face diff-added))
      loaded-only baseline stacked restored result)
  (unwind-protect
      (progn
        (neomacs-zenburn-test--cleanup nil)
        (require 'ansi-color)
        (require 'diff-mode)
        (eval
         '(deftheme neomacs-zenburn-baseline
            "Theme already used before Zenburn."))
        (custom-theme-set-faces
         'neomacs-zenburn-baseline
         '(default ((t (:foreground "#e6edf3" :background "#202830"))))
         '(cursor ((t (:foreground "#202830" :background "#f0c674"))))
         '(region ((t (:background "#405060" :extend t))))
         '(isearch ((t (:foreground "#202830" :background "#f0c674"))))
         '(font-lock-comment-face ((t (:foreground "#708090" :slant italic))))
         '(font-lock-keyword-face ((t (:foreground "#c678dd" :weight bold))))
         '(diff-added ((t (:foreground "#98c379" :background "#243524")))))
        (custom-theme-set-variables
         'neomacs-zenburn-baseline
         '(ansi-color-names-vector
           ["#202830" "#e06c75" "#98c379" "#e5c07b"
            "#61afef" "#c678dd" "#56b6c2" "#e6edf3"]))
        (provide-theme 'neomacs-zenburn-baseline)
        (enable-theme 'neomacs-zenburn-baseline)
        (neomacs-zenburn-test--reload :no-enable t)
        (setq loaded-only
              (list
               :known (and (custom-theme-p 'zenburn) t)
               :enabled (and (custom-theme-enabled-p 'zenburn) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'zenburn 'theme-settings))
               :faces (neomacs-zenburn-test--face-state faces)
               :ansi (append ansi-color-names-vector nil)))
        (setq baseline loaded-only)
        (enable-theme 'zenburn)
        (setq stacked
              (list
               :themes (copy-sequence custom-enabled-themes)
               :zenburn (and (custom-theme-enabled-p 'zenburn) t)
               :baseline
               (and (custom-theme-enabled-p 'neomacs-zenburn-baseline) t)
               :faces (neomacs-zenburn-test--face-state faces)
               :ansi (append ansi-color-names-vector nil)))
        (disable-theme 'zenburn)
        (setq restored
              (list
               :themes (copy-sequence custom-enabled-themes)
               :zenburn (and (custom-theme-enabled-p 'zenburn) t)
               :known (and (custom-theme-p 'zenburn) t)
               :faces (neomacs-zenburn-test--face-state faces)
               :ansi (append ansi-color-names-vector nil)))
        (setq result
              (list
               :loaded-only loaded-only
               :stacked stacked
               :restored restored
               :restored-baseline
               (and
                (equal (plist-get baseline :faces)
                       (plist-get restored :faces))
                (equal (plist-get baseline :ansi)
                       (plist-get restored :ansi)))
               :zenburn-changed
               (not (equal (plist-get baseline :faces)
                           (plist-get stacked :faces))))))
    (neomacs-zenburn-test--cleanup nil))
  result)
"####;
    let expect = expect![[
        r####"OK (:loaded-only (:known t :enabled nil :themes (neomacs-zenburn-baseline) :settings 1567 :faces ((default "#e6edf3" "#202830" "#e6edf3" "#202830" normal normal nil nil nil) (cursor "#202830" "#f0c674" "#202830" "#f0c674" unspecified unspecified unspecified unspecified unspecified) (region unspecified "#405060" "#e6edf3" "#405060" unspecified unspecified unspecified t unspecified) (isearch "#202830" "#f0c674" "#202830" "#f0c674" unspecified unspecified unspecified unspecified unspecified) (font-lock-comment-face "#708090" unspecified "#708090" "#202830" unspecified italic unspecified unspecified unspecified) (font-lock-keyword-face "#c678dd" unspecified "#c678dd" "#202830" bold unspecified unspecified unspecified unspecified) (diff-added "#98c379" "#243524" "#98c379" "#243524" unspecified unspecified unspecified unspecified unspecified)) :ansi ("#202830" "#e06c75" "#98c379" "#e5c07b" "#61afef" "#c678dd" "#56b6c2" "#e6edf3")) :stacked (:themes (zenburn neomacs-zenburn-baseline) :zenburn t :baseline t :faces ((default "#DCDCCC" "#3F3F3F" "#DCDCCC" "#3F3F3F" normal normal nil nil nil) (cursor "#DCDCCC" "#FFFFEF" "#DCDCCC" "#FFFFEF" unspecified unspecified unspecified unspecified unspecified) (region unspecified "#2B2B2B" "#DCDCCC" "#2B2B2B" unspecified unspecified unspecified t unspecified) (isearch "#D0BF8F" "#5F5F5F" "#D0BF8F" "#5F5F5F" bold unspecified unspecified unspecified unspecified) (font-lock-comment-face "#7F9F7F" unspecified "#7F9F7F" "#3F3F3F" unspecified italic unspecified unspecified unspecified) (font-lock-keyword-face "#F0DFAF" unspecified "#F0DFAF" "#3F3F3F" bold unspecified unspecified unspecified unspecified) (diff-added "#7F9F7F" "#335533" "#7F9F7F" "#335533" unspecified unspecified unspecified unspecified unspecified)) :ansi ("#3F3F3F" "#CC9393" "#7F9F7F" "#F0DFAF" "#8CD0D3" "#DC8CC3" "#93E0E3" "#DCDCCC")) :restored (:themes (neomacs-zenburn-baseline) :zenburn nil :known t :faces ((default "#e6edf3" "#202830" "#e6edf3" "#202830" normal normal nil nil nil) (cursor "#202830" "#f0c674" "#202830" "#f0c674" unspecified unspecified unspecified unspecified unspecified) (region unspecified "#405060" "#e6edf3" "#405060" unspecified unspecified unspecified t unspecified) (isearch "#202830" "#f0c674" "#202830" "#f0c674" unspecified unspecified unspecified unspecified unspecified) (font-lock-comment-face "#708090" unspecified "#708090" "#202830" unspecified italic unspecified unspecified unspecified) (font-lock-keyword-face "#c678dd" unspecified "#c678dd" "#202830" bold unspecified unspecified unspecified unspecified) (diff-added "#98c379" "#243524" "#98c379" "#243524" unspecified unspecified unspecified unspecified unspecified)) :ansi ("#202830" "#e06c75" "#98c379" "#e5c07b" "#61afef" "#c678dd" "#56b6c2" "#e6edf3")) :restored-baseline t :zenburn-changed t)"####
    ]];
    ParityBatchCase::value(
        "disabling_zenburn_restores_a_preexisting_theme_and_variables",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn diagnostic_fallbacks_and_package_integrations_keep_their_exact_contracts() -> ParityBatchCase {
    let elisp_form = r####"
(let ((diagnostics '(flymake-error flymake-warning flymake-note))
      rendered supported fallback result)
  (unwind-protect
      (progn
        (require 'flymake)
        (neomacs-zenburn-test--reload)
        (with-temp-buffer
          (dolist (entry
                   '(("missing release" . flymake-error)
                     ("deprecated flag" . flymake-warning)
                     ("consider retry" . flymake-note)))
            (insert (propertize (car entry) 'face (cdr entry)) "\n"))
          (setq rendered
                (list
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :runs (neomacs-zenburn-test--property-runs 'face))))
        (setq supported
              (cl-letf
                  (((symbol-function 'display-supports-face-attributes-p)
                    (lambda (&rest _arguments) t)))
                (mapcar
                 (lambda (face)
                   (list
                    face
                    (copy-tree
                     (face-spec-choose
                      (cadr (assq 'zenburn (get face 'theme-face)))))))
                 diagnostics))
              fallback
              (cl-letf
                  (((symbol-function 'display-supports-face-attributes-p)
                    (lambda (&rest _arguments) nil)))
                (mapcar
                 (lambda (face)
                   (list
                    face
                    (copy-tree
                     (face-spec-choose
                      (cadr (assq 'zenburn (get face 'theme-face)))))))
                 diagnostics)))
        (setq result
              (list
               :wave-supported
               (and
                (display-supports-face-attributes-p
                 '(:underline (:style wave)) nil)
                t)
               :rendered rendered
               :faces (neomacs-zenburn-test--face-state diagnostics)
               :selected
               (mapcar
                (lambda (face)
                  (list
                   face
                   (copy-tree
                    (face-spec-choose
                     (cadr (assq 'zenburn (get face 'theme-face)))))))
                diagnostics)
               :supported supported
               :fallback fallback
               :integration-specs
               (neomacs-zenburn-test--theme-face-specs
                '(gptel-context-deletion-face gptel-response-highlight
                  magit-diff-added vundo-diff-highlight
                  asciidoc-document-title-face easy-kill-selection
                  dictionary-reference-face))
               :settings
               (list
                :total (length (get 'zenburn 'theme-settings))
                :faces
                (cl-count 'theme-face (get 'zenburn 'theme-settings)
                          :key #'car)
                :variables
                (cl-count 'theme-value (get 'zenburn 'theme-settings)
                          :key #'car)))))
    (neomacs-zenburn-test--cleanup nil))
  result)
"####;
    let expect = expect![[
        r####"OK (:wave-supported nil :rendered (:text "missing release\ndeprecated flag\nconsider retry\n" :runs (("missing release" flymake-error 1 16) ("deprecated flag" flymake-warning 17 32) ("consider retry" flymake-note 33 47))) :faces ((flymake-error "#BC8383" unspecified "#BC8383" "#3F3F3F" bold unspecified t unspecified unspecified) (flymake-warning "#DFAF8F" unspecified "#DFAF8F" "#3F3F3F" bold unspecified t unspecified unspecified) (flymake-note "#5F7F5F" unspecified "#5F7F5F" "#3F3F3F" bold unspecified t unspecified unspecified)) :selected ((flymake-error (:foreground "#BC8383" :weight bold :underline t)) (flymake-warning (:foreground "#DFAF8F" :weight bold :underline t)) (flymake-note (:foreground "#5F7F5F" :weight bold :underline t))) :supported ((flymake-error (:underline (:style wave :color "#CC9393") :inherit unspecified :foreground unspecified :background unspecified)) (flymake-warning (:underline (:style wave :color "#DFAF8F") :inherit unspecified :foreground unspecified :background unspecified)) (flymake-note (:underline (:style wave :color "#7F9F7F") :inherit unspecified :foreground unspecified :background unspecified))) :fallback ((flymake-error (:foreground "#BC8383" :weight bold :underline t)) (flymake-warning (:foreground "#DFAF8F" :weight bold :underline t)) (flymake-note (:foreground "#5F7F5F" :weight bold :underline t))) :integration-specs ((gptel-context-deletion-face ((t (:background "#553333" :extend t)))) (gptel-response-highlight ((t (:background "#383838" :extend t)))) (magit-diff-added ((t (:background "#5F7F5F")))) (vundo-diff-highlight ((t (:foreground "#DFAF8F" :weight bold)))) (asciidoc-document-title-face ((t (:inherit adoc-title-0-face)))) (easy-kill-selection ((t (:background "#2B2B2B" :extend t)))) (dictionary-reference-face ((t (:inherit link))))) :settings (:total 1567 :faces 1561 :variables 6))"####
    ]];
    ParityBatchCase::value(
        "diagnostic_fallbacks_and_package_integrations_keep_their_exact_contracts",
        elisp_form,
        expect,
    )
}

fn invalid_semantic_palette_reference_fails_during_theme_loading() -> ParityBatchCase {
    let elisp_form = r####"
(let ((zenburn-override-colors-alist nil)
      (zenburn-override-semantic-colors-alist
       '(("zenburn-error" . neomacs-missing-palette-color)))
      (zenburn-use-variable-pitch nil)
      (zenburn-scale-org-headlines nil)
      (zenburn-scale-outline-headlines nil))
  (when (custom-theme-enabled-p 'zenburn)
    (disable-theme 'zenburn))
  (load-theme 'zenburn t))
"####;
    let expect = expect![[r####"ERR (void-variable neomacs-missing-palette-color)"####]];
    ParityBatchCase::signal(
        "invalid_semantic_palette_reference_fails_during_theme_loading",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(crate) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loads_the_theme_into_a_real_elisp_release_review(),
        custom_palette_layers_repaint_real_source_and_diff_buffers(),
        scaled_variable_pitch_headings_survive_a_live_org_reload(),
        theme_variables_colorize_a_real_terminal_build_log(),
        rainbow_mode_toggles_zenburn_palette_names_in_real_lisp_buffers(),
        late_defined_package_faces_receive_the_enabled_theme_in_real_content(),
        disabling_zenburn_restores_a_preexisting_theme_and_variables(),
        diagnostic_fallbacks_and_package_integrations_keep_their_exact_contracts(),
        invalid_semantic_palette_reference_fails_during_theme_loading(),
    ]
}
