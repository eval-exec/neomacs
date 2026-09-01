use expect_test::expect;

use super::ParityBatchCase;

fn all_five_variants_apply_distinct_complete_palettes_and_variables() -> ParityBatchCase {
    ParityBatchCase::value(
        "all_five_variants_apply_distinct_complete_palettes_and_variables",
        r####"
(unwind-protect
    (mapcar
     (lambda (variant)
       (neomacs-tomorrow-test-load variant)
       (let ((theme (color-theme-sanityinc-tomorrow--theme-name variant)))
         (list variant
               :enabled (copy-sequence custom-enabled-themes)
               :settings (neomacs-tomorrow-test-theme-settings theme)
               :specs (neomacs-tomorrow-test-theme-face-specs
                       theme
                       '(default cursor fringe region mode-line
                         font-lock-keyword-face font-lock-string-face
                         font-lock-comment-face error warning success))
               :variables
               (list :background-mode frame-background-mode
                     :ansi (append ansi-color-names-vector nil)
                     :fci (and (boundp 'fci-rule-color) fci-rule-color)
                     :divider window-divider-mode
                     :vc-head (seq-take vc-annotate-color-map 4)))))
     '(night day eighties blue bright))
  (neomacs-tomorrow-test-cleanup))
"####,
        expect![[
            r##"OK ((night :enabled (sanityinc-tomorrow-night) :settings (:faces 1191 :variables 9 :immediate t) :specs ((default ((((class color) (min-colors 89)) (:foreground "#c5c8c6" :background "#1d1f21")))) (cursor ((((class color) (min-colors 89)) (:background "#cc6666")))) (fringe ((((class color) (min-colors 89)) (:background "#22a224a427a7" :foreground "#969896")))) (region ((((class color) (min-colors 89)) (:background "#373b41" :inverse-video nil :extend t)))) (mode-line ((((class color) (min-colors 89)) (:foreground "#c5c8c6" :background "#373b41" :weight normal :box (:line-width 1 :color "#373b41"))))) (font-lock-keyword-face ((((class color) (min-colors 89)) (:foreground "#b5bd68")))) (font-lock-string-face ((((class color) (min-colors 89)) (:foreground "#8abeb7")))) (font-lock-comment-face ((((class color) (min-colors 89)) (:foreground "#969896")))) (error ((((class color) (min-colors 89)) (:foreground "#cc6666")))) (warning ((((class color) (min-colors 89)) (:foreground "#de935f")))) (success ((((class color) (min-colors 89)) (:foreground "#b5bd68"))))) :variables (:background-mode dark :ansi ("#1d1f21" "#cc6666" "#b5bd68" "#f0c674" "#81a2be" "#b294bb" "#8abeb7" "#c5c8c6") :fci nil :divider nil :vc-head ((20 . "#cc6666") (40 . "#de935f") (60 . "#f0c674") (80 . "#b5bd68")))) (day :enabled (sanityinc-tomorrow-day) :settings (:faces 1191 :variables 9 :immediate t) :specs ((default ((((class color) (min-colors 89)) (:foreground "#4d4d4c" :background "#ffffff")))) (cursor ((((class color) (min-colors 89)) (:background "#c82829")))) (fringe ((((class color) (min-colors 89)) (:background "#f7f7f7f7f7f7" :foreground "#8e908c")))) (region ((((class color) (min-colors 89)) (:background "#d6d6d6" :inverse-video nil :extend t)))) (mode-line ((((class color) (min-colors 89)) (:foreground "#4d4d4c" :background "#d6d6d6" :weight normal :box (:line-width 1 :color "#d6d6d6"))))) (font-lock-keyword-face ((((class color) (min-colors 89)) (:foreground "#718c00")))) (font-lock-string-face ((((class color) (min-colors 89)) (:foreground "#3e999f")))) (font-lock-comment-face ((((class color) (min-colors 89)) (:foreground "#8e908c")))) (error ((((class color) (min-colors 89)) (:foreground "#c82829")))) (warning ((((class color) (min-colors 89)) (:foreground "#f5871f")))) (success ((((class color) (min-colors 89)) (:foreground "#718c00"))))) :variables (:background-mode light :ansi ("#ffffff" "#c82829" "#718c00" "#eab700" "#4271ae" "#8959a8" "#3e999f" "#4d4d4c") :fci nil :divider nil :vc-head ((20 . "#c82829") (40 . "#f5871f") (60 . "#eab700") (80 . "#718c00")))) (eighties :enabled (sanityinc-tomorrow-eighties) :settings (:faces 1191 :variables 9 :immediate t) :specs ((default ((((class color) (min-colors 89)) (:foreground "#cccccc" :background "#2d2d2d")))) (cursor ((((class color) (min-colors 89)) (:background "#f2777a")))) (fringe ((((class color) (min-colors 89)) (:background "#333333333333" :foreground "#999999")))) (region ((((class color) (min-colors 89)) (:background "#515151" :inverse-video nil :extend t)))) (mode-line ((((class color) (min-colors 89)) (:foreground "#cccccc" :background "#515151" :weight normal :box (:line-width 1 :color "#515151"))))) (font-lock-keyword-face ((((class color) (min-colors 89)) (:foreground "#99cc99")))) (font-lock-string-face ((((class color) (min-colors 89)) (:foreground "#66cccc")))) (font-lock-comment-face ((((class color) (min-colors 89)) (:foreground "#999999")))) (error ((((class color) (min-colors 89)) (:foreground "#f2777a")))) (warning ((((class color) (min-colors 89)) (:foreground "#f99157")))) (success ((((class color) (min-colors 89)) (:foreground "#99cc99"))))) :variables (:background-mode dark :ansi ("#2d2d2d" "#f2777a" "#99cc99" "#ffcc66" "#6699cc" "#cc99cc" "#66cccc" "#cccccc") :fci nil :divider nil :vc-head ((20 . "#f2777a") (40 . "#f99157") (60 . "#ffcc66") (80 . "#99cc99")))) (blue :enabled (sanityinc-tomorrow-blue) :settings (:faces 1191 :variables 9 :immediate t) :specs ((default ((((class color) (min-colors 89)) (:foreground "#ffffff" :background "#002451")))) (cursor ((((class color) (min-colors 89)) (:background "#ff9da4")))) (fringe ((((class color) (min-colors 89)) (:background "#00002c2c5fdf" :foreground "#7285b7")))) (region ((((class color) (min-colors 89)) (:background "#003f8e" :inverse-video nil :extend t)))) (mode-line ((((class color) (min-colors 89)) (:foreground "#ffffff" :background "#003f8e" :weight normal :box (:line-width 1 :color "#003f8e"))))) (font-lock-keyword-face ((((class color) (min-colors 89)) (:foreground "#d1f1a9")))) (font-lock-string-face ((((class color) (min-colors 89)) (:foreground "#99ffff")))) (font-lock-comment-face ((((class color) (min-colors 89)) (:foreground "#7285b7")))) (error ((((class color) (min-colors 89)) (:foreground "#ff9da4")))) (warning ((((class color) (min-colors 89)) (:foreground "#ffc58f")))) (success ((((class color) (min-colors 89)) (:foreground "#d1f1a9"))))) :variables (:background-mode dark :ansi ("#002451" "#ff9da4" "#d1f1a9" "#ffeead" "#bbdaff" "#ebbbff" "#99ffff" "#ffffff") :fci nil :divider nil :vc-head ((20 . "#ff9da4") (40 . "#ffc58f") (60 . "#ffeead") (80 . "#d1f1a9")))) (bright :enabled (sanityinc-tomorrow-bright) :settings (:faces 1191 :variables 9 :immediate t) :specs ((default ((((class color) (min-colors 89)) (:foreground "#eaeaea" :background "#000000")))) (cursor ((((class color) (min-colors 89)) (:background "#d54e53")))) (fringe ((((class color) (min-colors 89)) (:background "#151515151515" :foreground "#969896")))) (region ((((class color) (min-colors 89)) (:background "#424242" :inverse-video nil :extend t)))) (mode-line ((((class color) (min-colors 89)) (:foreground "#eaeaea" :background "#424242" :weight normal :box (:line-width 1 :color "#424242"))))) (font-lock-keyword-face ((((class color) (min-colors 89)) (:foreground "#b9ca4a")))) (font-lock-string-face ((((class color) (min-colors 89)) (:foreground "#70c0b1")))) (font-lock-comment-face ((((class color) (min-colors 89)) (:foreground "#969896")))) (error ((((class color) (min-colors 89)) (:foreground "#d54e53")))) (warning ((((class color) (min-colors 89)) (:foreground "#e78c45")))) (success ((((class color) (min-colors 89)) (:foreground "#b9ca4a"))))) :variables (:background-mode dark :ansi ("#000000" "#d54e53" "#b9ca4a" "#e7c547" "#7aa6da" "#c397d8" "#70c0b1" "#eaeaea") :fci nil :divider nil :vc-head ((20 . "#d54e53") (40 . "#e78c45") (60 . "#e7c547") (80 . "#b9ca4a")))))"##
        ]],
    )
}

fn wrapper_commands_switch_variants_and_replace_the_previously_enabled_theme() -> ParityBatchCase {
    ParityBatchCase::value(
        "wrapper_commands_switch_variants_and_replace_the_previously_enabled_theme",
        r####"
(unwind-protect
    (let (states)
      ;; Wrapper commands select already-known themes, matching the documented
      ;; workflow after requiring this package or opening customize-themes.
      (dolist (variant '(night day eighties blue bright))
        (load-theme (color-theme-sanityinc-tomorrow--theme-name variant) t t))
      (dolist (command '(color-theme-sanityinc-tomorrow-night
                         color-theme-sanityinc-tomorrow-day
                         color-theme-sanityinc-tomorrow-blue
                         color-theme-sanityinc-tomorrow-bright
                         color-theme-sanityinc-tomorrow-eighties))
        (funcall command)
        (push (list command
                    (copy-sequence custom-enabled-themes)
                    frame-background-mode
                    (face-attribute 'default :background nil 'default)
                    (face-attribute 'default :foreground nil 'default))
              states))
      (nreverse states))
  (neomacs-tomorrow-test-cleanup))
"####,
        expect![[
            r#"OK ((color-theme-sanityinc-tomorrow-night (sanityinc-tomorrow-night) dark "unspecified-bg" "unspecified-fg") (color-theme-sanityinc-tomorrow-day (sanityinc-tomorrow-day) light "unspecified-bg" "unspecified-fg") (color-theme-sanityinc-tomorrow-blue (sanityinc-tomorrow-blue) dark "unspecified-bg" "unspecified-fg") (color-theme-sanityinc-tomorrow-bright (sanityinc-tomorrow-bright) dark "unspecified-bg" "unspecified-fg") (color-theme-sanityinc-tomorrow-eighties (sanityinc-tomorrow-eighties) dark "unspecified-bg" "unspecified-fg"))"#
        ]],
    )
}

fn night_and_day_fontify_a_real_elisp_review_with_inverted_contrast() -> ParityBatchCase {
    ParityBatchCase::value(
        "night_and_day_fontify_a_real_elisp_review_with_inverted_contrast",
        r####"
(unwind-protect
    (mapcar
     (lambda (variant)
       (neomacs-tomorrow-test-load variant)
       (with-temp-buffer
         (emacs-lisp-mode)
         (insert ";; Review release Ω\n(defconst release-limit 42)\n(defun ship (name)\n  (if name (message \"ship %s\" name) (error \"missing\")))\n")
         (font-lock-ensure)
         (let ((theme (color-theme-sanityinc-tomorrow--theme-name variant)))
           (list variant
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :tokens (neomacs-tomorrow-test-token-state
                          '("Review release" "defconst" "release-limit" "42"
                            "defun" "ship" "if name" "\"ship %s\""
                            "error" "\"missing\""))
                 :specs (neomacs-tomorrow-test-theme-face-specs
                         theme
                         '(font-lock-comment-face font-lock-keyword-face
                           font-lock-variable-name-face
                           font-lock-function-name-face font-lock-string-face
                           font-lock-warning-face))))))
     '(night day))
  (neomacs-tomorrow-test-cleanup))
"####,
        expect![[
            r##"OK ((night :text ";; Review release Ω\n(defconst release-limit 42)\n(defun ship (name)\n  (if name (message \"ship %s\" name) (error \"missing\")))\n" :tokens (("Review release" 4 font-lock-comment-face "unspecified-fg" "unspecified-bg" bold) ("defconst" 22 font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("release-limit" 31 font-lock-variable-name-face "unspecified-fg" "unspecified-bg" bold) ("42" 45 nil nil nil nil) ("defun" 50 font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("ship" 56 font-lock-function-name-face "unspecified-fg" "unspecified-bg" bold) ("if name" 71 font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("\"ship %s\"" 88 font-lock-string-face "unspecified-fg" "unspecified-bg" unspecified) ("error" 105 font-lock-warning-face "unspecified-fg" "unspecified-bg" bold) ("\"missing\"" 111 font-lock-string-face "unspecified-fg" "unspecified-bg" unspecified)) :specs ((font-lock-comment-face ((((class color) (min-colors 89)) (:foreground "#969896")))) (font-lock-keyword-face ((((class color) (min-colors 89)) (:foreground "#b5bd68")))) (font-lock-variable-name-face ((((class color) (min-colors 89)) (:foreground "#f0c674")))) (font-lock-function-name-face ((((class color) (min-colors 89)) (:foreground "#de935f")))) (font-lock-string-face ((((class color) (min-colors 89)) (:foreground "#8abeb7")))) (font-lock-warning-face ((((class color) (min-colors 89)) (:weight bold :foreground "#cc6666")))))) (day :text ";; Review release Ω\n(defconst release-limit 42)\n(defun ship (name)\n  (if name (message \"ship %s\" name) (error \"missing\")))\n" :tokens (("Review release" 4 font-lock-comment-face "unspecified-fg" "unspecified-bg" bold) ("defconst" 22 font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("release-limit" 31 font-lock-variable-name-face "unspecified-fg" "unspecified-bg" bold) ("42" 45 nil nil nil nil) ("defun" 50 font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("ship" 56 font-lock-function-name-face "unspecified-fg" "unspecified-bg" bold) ("if name" 71 font-lock-keyword-face "unspecified-fg" "unspecified-bg" bold) ("\"ship %s\"" 88 font-lock-string-face "unspecified-fg" "unspecified-bg" unspecified) ("error" 105 font-lock-warning-face "unspecified-fg" "unspecified-bg" bold) ("\"missing\"" 111 font-lock-string-face "unspecified-fg" "unspecified-bg" unspecified)) :specs ((font-lock-comment-face ((((class color) (min-colors 89)) (:foreground "#8e908c")))) (font-lock-keyword-face ((((class color) (min-colors 89)) (:foreground "#718c00")))) (font-lock-variable-name-face ((((class color) (min-colors 89)) (:foreground "#eab700")))) (font-lock-function-name-face ((((class color) (min-colors 89)) (:foreground "#f5871f")))) (font-lock-string-face ((((class color) (min-colors 89)) (:foreground "#3e999f")))) (font-lock-warning-face ((((class color) (min-colors 89)) (:weight bold :foreground "#c82829")))))))"##
        ]],
    )
}

fn bright_variant_drives_real_diff_and_ansi_color_workflows() -> ParityBatchCase {
    ParityBatchCase::value(
        "bright_variant_drives_real_diff_and_ansi_color_workflows",
        r####"
(unwind-protect
    (progn
      (require 'ansi-color)
      (require 'diff-mode)
      (neomacs-tomorrow-test-load 'bright)
      (list
       :ansi
       (with-temp-buffer
         (insert (ansi-color-apply "build \e[31mFAILED\e[0m, \e[32m42 passed\e[0m, \e[34mretry\e[0m\n"))
         (list :text (buffer-substring-no-properties (point-min) (point-max))
               :runs
               (let ((pos (point-min)) result)
                 (while (< pos (point-max))
                   (let* ((face (get-text-property pos 'font-lock-face))
                          (next (next-single-property-change pos 'font-lock-face nil (point-max))))
                     (when face
                       (push (list (buffer-substring-no-properties pos next) face) result))
                     (setq pos next)))
                 (nreverse result))))
       :diff
       (with-temp-buffer
         (insert "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-failed\n+ready Ω\n")
         (diff-mode)
         (font-lock-ensure)
         (list :text (buffer-substring-no-properties (point-min) (point-max))
               :tokens (neomacs-tomorrow-test-token-state
                        '("diff --git" "--- a/x" "+++ b/x" "@@ -1 +1 @@" "-failed" "+ready Ω"))))
       :specs (neomacs-tomorrow-test-theme-face-specs
               'sanityinc-tomorrow-bright
               '(ansi-color-red ansi-color-green ansi-color-blue
                 diff-added diff-removed diff-hunk-header))))
  (neomacs-tomorrow-test-cleanup))
"####,
        expect![[
            r##"OK (:ansi (:text "build FAILED, 42 passed, retry\n" :runs (("FAILED" (:foreground "red3")) ("42 passed" (:foreground "green3")) ("retry" (:foreground "blue2")))) :diff (:text "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-failed\n+ready Ω\n" :tokens (("diff --git" 1 diff-header "unspecified-fg" "unspecified-bg" bold) ("--- a/x" 20 diff-header "unspecified-fg" "unspecified-bg" bold) ("+++ b/x" 28 diff-header "unspecified-fg" "unspecified-bg" bold) ("@@ -1 +1 @@" 36 diff-hunk-header "unspecified-fg" "unspecified-bg" bold) ("-failed" 48 diff-indicator-removed "unspecified-fg" "unspecified-bg" unspecified) ("+ready Ω" 56 diff-indicator-added "unspecified-fg" "unspecified-bg" unspecified))) :specs ((ansi-color-red ((((class color) (min-colors 89)) (:foreground "#d54e53" :background "#d54e53")))) (ansi-color-green ((((class color) (min-colors 89)) (:foreground "#b9ca4a" :background "#b9ca4a")))) (ansi-color-blue ((((class color) (min-colors 89)) (:foreground "#7aa6da" :background "#7aa6da")))) (diff-added ((((class color) (min-colors 89)) (:foreground "#b9ca4a" :extend t)))) (diff-removed ((((class color) (min-colors 89)) (:foreground "#e78c45" :extend t)))) (diff-hunk-header ((((class color) (min-colors 89)) (:foreground "#c397d8"))))))"##
        ]],
    )
}

fn palette_helpers_theme_registry_and_invalid_flavor_errors_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "palette_helpers_theme_registry_and_invalid_flavor_errors_are_exact",
        r####"
(let ((registered
       (mapcar
        (lambda (variant)
          (let ((theme (color-theme-sanityinc-tomorrow--theme-name variant)))
            (load-theme theme t t)
            (list variant theme (custom-theme-p theme)
                  (get theme 'theme-immediate)
                  (neomacs-tomorrow-test-theme-settings theme))))
        '(night day eighties blue bright))))
  (list
   :registered registered
   :names (mapcar #'color-theme-sanityinc-tomorrow--theme-name
                  '(night day eighties blue bright))
   :interpolation
   (list (sanityinc-tomorrow--interpolate "#000000" "#ffffff" 7 3)
         (sanityinc-tomorrow--alt-background "#1d1f21" "#282a2e"))
   :invalid
   (condition-case err
       (list :value
             (color-theme-sanityinc-tomorrow--with-colors 'missing background))
     (error (list :signal (car err) :message (error-message-string err))))))
"####,
        expect![[
            r##"OK (:registered ((night sanityinc-tomorrow-night #1=(sanityinc-tomorrow-night user changed) t (:faces 1191 :variables 9 :immediate t)) (day sanityinc-tomorrow-day #2=(sanityinc-tomorrow-day . #1#) t (:faces 1191 :variables 9 :immediate t)) (eighties sanityinc-tomorrow-eighties #3=(sanityinc-tomorrow-eighties . #2#) t (:faces 1191 :variables 9 :immediate t)) (blue sanityinc-tomorrow-blue #4=(sanityinc-tomorrow-blue . #3#) t (:faces 1191 :variables 9 :immediate t)) (bright sanityinc-tomorrow-bright (sanityinc-tomorrow-bright . #4#) t (:faces 1191 :variables 9 :immediate t))) :names (sanityinc-tomorrow-night sanityinc-tomorrow-day sanityinc-tomorrow-eighties sanityinc-tomorrow-blue sanityinc-tomorrow-bright) :interpolation ("#7fff7fff7fff" "#000000000000") :invalid (:signal error :message "no such theme flavor"))"##
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        all_five_variants_apply_distinct_complete_palettes_and_variables(),
        wrapper_commands_switch_variants_and_replace_the_previously_enabled_theme(),
        night_and_day_fontify_a_real_elisp_review_with_inverted_contrast(),
        bright_variant_drives_real_diff_and_ansi_color_workflows(),
        palette_helpers_theme_registry_and_invalid_flavor_errors_are_exact(),
    ]
}
