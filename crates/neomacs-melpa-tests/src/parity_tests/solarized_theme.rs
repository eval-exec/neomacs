use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SOLARIZED_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'solarized-theme)
(require 'solarized-palettes)

(defconst neomacs-solarized-test-variants
  '(solarized-dark solarized-light
    solarized-dark-high-contrast solarized-light-high-contrast
    solarized-gruvbox-dark solarized-gruvbox-light
    solarized-selenized-black solarized-selenized-dark
    solarized-selenized-light solarized-selenized-white
    solarized-wombat-dark solarized-zenburn))

(defun neomacs-solarized-test-face-spec (theme face)
  "Return FACE's recorded face spec for THEME."
  (nth 3
       (cl-find-if
        (lambda (setting)
          (and (eq (nth 0 setting) 'theme-face)
               (eq (nth 1 setting) face)))
        (get theme 'theme-settings))))

(defun neomacs-solarized-test-face (theme face)
  "Return selected stable attributes from FACE in THEME."
  (list
   face
   (mapcar
    (lambda (clause)
      (let ((attributes (cadr clause)))
        (list :display (car clause)
              :foreground (plist-get attributes :foreground)
              :background (plist-get attributes :background)
              :inherit (plist-get attributes :inherit)
              :weight (plist-get attributes :weight)
              :slant (plist-get attributes :slant)
              :underline (plist-get attributes :underline)
              :overline (plist-get attributes :overline)
              :extend (plist-get attributes :extend)
              :inverse-video (plist-get attributes :inverse-video)
              :height (plist-get attributes :height)
              :family (plist-get attributes :family)
              :box (plist-get attributes :box))))
    (neomacs-solarized-test-face-spec theme face))))

(defun neomacs-solarized-test-color-name-to-rgb (color &optional _frame)
  "Parse deterministic six-digit COLOR values used by Solarized."
  (when (equal color "black") (setq color "#000000"))
  (unless (string-match
           "\\`#\\([[:xdigit:]]\\{2\\}\\)\\([[:xdigit:]]\\{2\\}\\)\\([[:xdigit:]]\\{2\\}\\)\\'"
           color)
    (error "Unsupported deterministic color: %s" color))
  (mapcar (lambda (index)
            (/ (string-to-number (match-string index color) 16) 255.0))
          '(1 2 3)))

(defun neomacs-solarized-test-load (theme)
  "Reload THEME without enabling it."
  (cl-letf (((symbol-function 'color-name-to-rgb)
             #'neomacs-solarized-test-color-name-to-rgb))
    (load-theme theme t t))
  theme)
"###;

fn package_contract_exposes_all_variants_palette_tools_and_policy_defaults() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'solarized-theme package-alist)))
      (available (custom-available-themes)))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :features (mapcar #'featurep
                           '(solarized-theme solarized solarized-faces
                             solarized-palettes)))
   :variants
   (mapcar (lambda (theme) (list theme (and (memq theme available) t)))
           neomacs-solarized-test-variants)
   :functions
   (mapcar #'fboundp
           '(solarized-color-clamp-lab solarized-color-rgb-to-hex
             solarized-color-blend solarized-create-color-palette))
   :defaults
   (list solarized-distinct-fringe-background
         solarized-distinct-doc-face solarized-highlight-numbers
         solarized-use-variable-pitch solarized-use-less-bold
         solarized-use-more-italic solarized-emphasize-indicators
         solarized-high-contrast-mode-line
         solarized-height-minus-1 solarized-height-plus-1
         solarized-height-plus-2 solarized-height-plus-3
         solarized-height-plus-4 solarized-scale-org-headlines
         solarized-scale-markdown-headlines
         solarized-scale-outline-headlines)))
"###;
    let expected = expect![[
        r#"OK (:package (:name solarized-theme :version "20260728.833" :requirements ((emacs (24 1))) :features (t t t t)) :variants ((solarized-dark t) (solarized-light t) (solarized-dark-high-contrast t) (solarized-light-high-contrast t) (solarized-gruvbox-dark t) (solarized-gruvbox-light t) (solarized-selenized-black t) (solarized-selenized-dark t) (solarized-selenized-light t) (solarized-selenized-white t) (solarized-wombat-dark t) (solarized-zenburn t)) :functions (t t t t) :defaults (nil nil nil t nil nil t nil 0.8 1.1 1.15 1.2 1.3 t nil t))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_all_variants_palette_tools_and_policy_defaults",
        elisp_form,
        expected,
    )
}

fn palette_data_and_lab_blending_produce_exact_reusable_color_systems() -> ParityBatchCase {
    let elisp_form = r###"
(let ((palettes
       `((dark . ,solarized-dark-color-palette-alist)
         (light . ,solarized-light-color-palette-alist)
         (dark-high . ,solarized-dark-high-contrast-palette-alist)
         (light-high . ,solarized-light-high-contrast-palette-alist)
         (gruvbox-dark . ,solarized-gruvbox-dark-color-palette-alist)
         (gruvbox-light . ,solarized-gruvbox-light-color-palette-alist)
         (zenburn . ,solarized-zenburn-color-palette-alist)
         (selenized-black . ,solarized-selenized-black-color-palette-alist)
         (selenized-dark . ,solarized-selenized-dark-color-palette-alist)
         (selenized-light . ,solarized-selenized-light-color-palette-alist)
         (selenized-white . ,solarized-selenized-white-color-palette-alist))))
  (list
   :palettes
   (mapcar
    (lambda (entry)
      (let ((palette (cdr entry)))
        (list (car entry) :count (length palette)
              :core (mapcar (lambda (name) (cons name (cdr (assq name palette))))
                            '(base03 base02 base0 base2 base3
                              yellow red blue cyan green)))))
    palettes)
   :hex
   (list (solarized-color-rgb-to-hex 0.0 0.5 1.0 2 t)
         (solarized-color-rgb-to-hex 0.0 0.5 1.0 4 nil))
   :clamp (solarized-color-clamp-lab '(-20 180 -190))
   :color-system
   (cl-letf (((symbol-function 'color-name-to-rgb)
              #'neomacs-solarized-test-color-name-to-rgb))
     (list
      :blends
      (mapcar (lambda (alpha)
                (list alpha
                      (solarized-color-blend "#002b36" "#fdf6e3" alpha 2)))
              '(0.0 0.25 0.5 0.75 1.0))
      :generated
      (let ((palette
             (solarized-create-color-palette
              '("#101820" "#f7f3e8" "#d6a700" "#dc6b2f" "#e34b4b"
                "#c45193" "#7a73d1" "#398bd5" "#35a69b" "#86a83c"))))
        (list :count (length palette)
              :first (cl-subseq palette 0 8)
              :last (last palette 8)))))))
"###;
    let expected = expect![[
        r##"OK (:palettes ((dark :count 64 :core ((base03 . "#002b36") (base02 . "#073642") (base0 . "#839496") (base2 . "#eee8d5") (base3 . "#fdf6e3") (yellow . "#b58900") (red . "#dc322f") (blue . "#268bd2") (cyan . "#2aa198") (green . "#859900"))) (light :count 64 :core ((base03 . "#002b36") (base02 . "#073642") (base0 . "#839496") (base2 . "#eee8d5") (base3 . "#fdf6e3") (yellow . "#b58900") (red . "#dc322f") (blue . "#268bd2") (cyan . "#2aa198") (green . "#859900"))) (dark-high :count 64 :core ((base03 . "#002732") (base02 . "#01323d") (base0 . "#8d9fa1") (base2 . "#faf3e0") (base3 . "#ffffee") (yellow . "#c49619") (red . "#ec423a") (blue . "#3c98e0") (cyan . "#3cafa5") (green . "#93a61a"))) (light-high :count 64 :core ((base03 . "#00212b") (base02 . "#002b37") (base0 . "#88999b") (base2 . "#f4eedb") (base3 . "#fffce9") (yellow . "#a67c00") (red . "#cc1f24") (blue . "#007ec4") (cyan . "#11948b") (green . "#778c00"))) (gruvbox-dark :count 64 :core ((base03 . "#282828") (base02 . "#32302f") (base0 . "#a89984") (base2 . "#a89984") (base3 . "#fbf1c7") (yellow . "#d79921") (red . "#fb4933") (blue . "#458588") (cyan . "#689d6a") (green . "#98971a"))) (gruvbox-light :count 64 :core ((base03 . "#282828") (base02 . "#32302f") (base0 . "#3c3836") (base2 . "#ebdbb2") (base3 . "#fbf1c7") (yellow . "#b57614") (red . "#9d0006") (blue . "#076678") (cyan . "#689d6a") (green . "#98971a"))) (zenburn :count 64 :core ((base03 . "#3F3F3F") (base02 . "#4F4F4F") (base0 . "#DCDCCC") (base2 . "#fffff6") (base3 . "#FFFFFD") (yellow . "#F0DFAF") (red . "#CC9393") (blue . "#8CD0D3") (cyan . "#93E0E3") (green . "#7F9F7F"))) (selenized-black :count 64 :core ((base03 . "#181818") (base02 . "#252525") (base0 . "#b9b9b9") (base2 . "#53676d") (base3 . "#3a4d53") (yellow . "#dbb32d") (red . "#ed4a46") (blue . "#368aeb") (cyan . "#3fc5b7") (green . "#70b433"))) (selenized-dark :count 64 :core ((base03 . "#103c48") (base02 . "#184956") (base0 . "#adbcbc") (base2 . "#ece3cc") (base3 . "#fbf3db") (yellow . "#dbb32d") (red . "#fa5750") (blue . "#4695f7") (cyan . "#41c7b9") (green . "#75b938"))) (selenized-light :count 64 :core ((base03 . "#fbf3db") (base02 . "#ece3cc") (base0 . "#53676d") (base2 . "#adbcbc") (base3 . "#cad8d9") (yellow . "#ad8900") (red . "#d2212d") (blue . "#0072d4") (cyan . "#009c8f") (green . "#489100"))) (selenized-white :count 64 :core ((base03 . "#ffffff") (base02 . "#ebebeb") (base0 . "#474747") (base2 . "#b9b9b9") (base3 . "#dedede") (yellow . "#c49700") (red . "#d6000c") (blue . "#0064e4") (cyan . "#00ad9c") (green . "#1d9700")))) :hex ("#0080ff" "#00007fffffff") :clamp (0.0 127 -128) :color-system (:blends ((0.0 "#fdf6e3") (0.25 "#bbbeb4") (0.5 "#7d8988") (0.75 "#41585d") (1.0 "#002b36")) :generated (:count 64 :first ((base03 . "#101820") (base02 . "#161d25") (base01 . "#585c5f") (base00 . "#636668") (base0 . "#7f8180") (base1 . "#8d8e8d") (base2 . "#e7e4da") (base3 . "#f7f3e8")) :last ((yellow-2fg . "#ebc87a") (orange-2fg . "#f1a880") (red-2fg . "#f59b8e") (magenta-2fg . "#de9cb9") (violet-2fg . "#b6abdc") (blue-2fg . "#9eb8de") (cyan-2fg . "#97c9bd") (green-2fg . "#bac98a")))))"##
    ]];
    ParityBatchCase::value(
        "palette_data_and_lab_blending_produce_exact_reusable_color_systems",
        elisp_form,
        expected,
    )
}

fn dark_and_light_themes_resolve_core_editor_and_programming_face_specs() -> ParityBatchCase {
    let elisp_form = r###"
(mapcar
 (lambda (theme)
   (neomacs-solarized-test-load theme)
   (list theme
         :settings (length (get theme 'theme-settings))
         :faces
         (mapcar (lambda (face) (neomacs-solarized-test-face theme face))
                 '(default cursor fringe region highlight
                   mode-line mode-line-inactive error warning success
                   font-lock-comment-face font-lock-doc-face
                   font-lock-function-name-face font-lock-keyword-face
                   font-lock-string-face font-lock-variable-name-face))))
 '(solarized-dark solarized-light))
"###;
    let expected = expect![[
        r##"OK ((solarized-dark :settings 1722 :faces ((default ((:display #1=((class color) (min-colors 89)) :foreground "#839496" :background "#002b36" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (cursor ((:display #1# :foreground "#002b36" :background "#839496" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video t :height nil :family nil :box nil))) (fringe ((:display #1# :foreground "#586e75" :background "#002b36" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (region ((:display #1# :foreground "#002b36" :background "#93a1a1" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend t :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#073642" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (mode-line ((:display #1# :foreground "#839496" :background "#073642" :inherit nil :weight nil :slant nil :underline "#284b54" :overline "#073642" :extend nil :inverse-video unspecified :height nil :family nil :box (:line-width 1 :color "#073642" :style nil)))) (mode-line-inactive ((:display #1# :foreground "#586e75" :background "#002b36" :inherit nil :weight nil :slant nil :underline "#284b54" :overline "#073642" :extend nil :inverse-video unspecified :height nil :family nil :box (:line-width 1 :color "#002b36" :style nil)))) (error ((:display #1# :foreground "#cb4b16" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (warning ((:display #1# :foreground "#b58900" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (success ((:display #1# :foreground "#859900" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#586e75" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-doc-face ((:display #1# :foreground "#2aa198" :background nil :inherit nil :weight nil :slant normal :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-function-name-face ((:display #1# :foreground "#268bd2" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#859900" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-string-face ((:display #1# :foreground "#2aa198" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-variable-name-face ((:display #1# :foreground "#268bd2" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))))) (solarized-light :settings 1722 :faces ((default ((:display #1# :foreground "#657b83" :background "#fdf6e3" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (cursor ((:display #1# :foreground "#fdf6e3" :background "#657b83" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video t :height nil :family nil :box nil))) (fringe ((:display #1# :foreground "#93a1a1" :background "#fdf6e3" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (region ((:display #1# :foreground "#fdf6e3" :background "#586e75" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend t :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#eee8d5" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (mode-line ((:display #1# :foreground "#657b83" :background "#eee8d5" :inherit nil :weight nil :slant nil :underline "#cccec4" :overline "#eee8d5" :extend nil :inverse-video unspecified :height nil :family nil :box (:line-width 1 :color "#eee8d5" :style nil)))) (mode-line-inactive ((:display #1# :foreground "#93a1a1" :background "#fdf6e3" :inherit nil :weight nil :slant nil :underline "#cccec4" :overline "#eee8d5" :extend nil :inverse-video unspecified :height nil :family nil :box (:line-width 1 :color "#fdf6e3" :style nil)))) (error ((:display #1# :foreground "#cb4b16" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (warning ((:display #1# :foreground "#b58900" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (success ((:display #1# :foreground "#859900" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#93a1a1" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-doc-face ((:display #1# :foreground "#2aa198" :background nil :inherit nil :weight nil :slant normal :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-function-name-face ((:display #1# :foreground "#268bd2" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#859900" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-string-face ((:display #1# :foreground "#2aa198" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-variable-name-face ((:display #1# :foreground "#268bd2" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))))))"##
    ]];
    ParityBatchCase::value(
        "dark_and_light_themes_resolve_core_editor_and_programming_face_specs",
        elisp_form,
        expected,
    )
}

fn user_options_materially_change_fringe_docs_numbers_typography_and_mode_line() -> ParityBatchCase
{
    let elisp_form = r###"
(let ((solarized-distinct-fringe-background t)
      (solarized-distinct-doc-face t)
      (solarized-highlight-numbers t)
      (solarized-use-variable-pitch nil)
      (solarized-use-less-bold t)
      (solarized-use-more-italic t)
      (solarized-high-contrast-mode-line t)
      (solarized-scale-org-headlines nil)
      (solarized-scale-outline-headlines nil))
  (neomacs-solarized-test-load 'solarized-dark)
  (list
   :faces
   (mapcar (lambda (face) (neomacs-solarized-test-face 'solarized-dark face))
           '(fringe font-lock-doc-face font-lock-number-face
             font-lock-builtin-face font-lock-keyword-face
             mode-line mode-line-inactive org-level-1 outline-1))
   :theme-enabled (custom-theme-enabled-p 'solarized-dark)
   :settings (length (get 'solarized-dark 'theme-settings))))
"###;
    let expected = expect![[
        r##"OK (:faces ((fringe ((:display #1=((class color) (min-colors 89)) :foreground "#586e75" :background "#073642" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-doc-face ((:display #1# :foreground "#6c71c4" :background nil :inherit nil :weight nil :slant italic :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-number-face ((:display #1# :foreground "#6c71c4" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-builtin-face ((:display #1# :foreground "#839496" :background nil :inherit nil :weight unspecified :slant italic :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#859900" :background nil :inherit nil :weight unspecified :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (mode-line ((:display #1# :foreground "#002b36" :background "#839496" :inherit nil :weight nil :slant nil :underline nil :overline "#839496" :extend nil :inverse-video unspecified :height nil :family nil :box (:line-width 1 :color "#839496" :style nil)))) (mode-line-inactive ((:display #1# :foreground "#839496" :background "#073642" :inherit nil :weight nil :slant nil :underline nil :overline "#073642" :extend nil :inverse-video unspecified :height nil :family nil :box (:line-width 1 :color "#073642" :style nil)))) (org-level-1 ((:display #1# :foreground "#cb4b16" :background nil :inherit default :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (outline-1 ((:display #1# :foreground "#cb4b16" :background nil :inherit default :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) :theme-enabled nil :settings 1722)"##
    ]];
    ParityBatchCase::value(
        "user_options_materially_change_fringe_docs_numbers_typography_and_mode_line",
        elisp_form,
        expected,
    )
}

fn every_shipped_variant_preserves_its_distinct_background_accent_and_comment_palette()
-> ParityBatchCase {
    let elisp_form = r###"
(mapcar
 (lambda (theme)
   (neomacs-solarized-test-load theme)
   (list theme
         (neomacs-solarized-test-face theme 'default)
         (neomacs-solarized-test-face theme 'font-lock-comment-face)
         (neomacs-solarized-test-face theme 'font-lock-keyword-face)
         (neomacs-solarized-test-face theme 'highlight)))
 neomacs-solarized-test-variants)
"###;
    let expected = expect![[
        r##"OK ((solarized-dark (default ((:display #1=((class color) (min-colors 89)) :foreground "#839496" :background "#002b36" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#586e75" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#859900" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#073642" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-light (default ((:display #1# :foreground "#657b83" :background "#fdf6e3" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#93a1a1" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#859900" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#eee8d5" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-dark-high-contrast (default ((:display #1# :foreground "#8d9fa1" :background "#002732" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#62787f" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#93a61a" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#01323d" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-light-high-contrast (default ((:display #1# :foreground "#596e76" :background "#fffce9" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#98a6a6" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#778c00" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#f4eedb" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-gruvbox-dark (default ((:display #1# :foreground "#a89984" :background "#282828" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#7c6f64" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#98971a" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#32302f" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-gruvbox-light (default ((:display #1# :foreground "#7c6f64" :background "#fbf1c7" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#a89984" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#98971a" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#ebdbb2" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-selenized-black (default ((:display #1# :foreground "#b9b9b9" :background "#181818" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#777777" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#70b433" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#252525" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-selenized-dark (default ((:display #1# :foreground "#adbcbc" :background "#103c48" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#72898f" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#75b938" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#184956" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-selenized-light (default ((:display #1# :foreground "#53676d" :background "#fbf3db" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#909995" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#489100" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#ece3cc" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-selenized-white (default ((:display #1# :foreground "#474747" :background "#ffffff" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#878787" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#1d9700" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#ebebeb" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-wombat-dark (default ((:display #1# :foreground "#8d8b86" :background "#2a2a29" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#6a6a65" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#8ac6f2" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#2f2f2e" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))) (solarized-zenburn (default ((:display #1# :foreground "#DCDCCC" :background "#3F3F3F" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-comment-face ((:display #1# :foreground "#878777" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (font-lock-keyword-face ((:display #1# :foreground "#7F9F7F" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (highlight ((:display #1# :foreground nil :background "#4F4F4F" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil)))))"##
    ]];
    ParityBatchCase::value(
        "every_shipped_variant_preserves_its_distinct_background_accent_and_comment_palette",
        elisp_form,
        expected,
    )
}

fn practical_diff_org_search_completion_and_diagnostic_faces_share_the_palette() -> ParityBatchCase
{
    let elisp_form = r###"
(progn
  (neomacs-solarized-test-load 'solarized-dark)
  (mapcar
   (lambda (face) (neomacs-solarized-test-face 'solarized-dark face))
   '(diff-added diff-removed diff-changed diff-refine-added diff-refine-removed
     org-level-1 org-level-2 org-todo org-done org-block
     isearch lazy-highlight match
     completions-common-part completions-annotations
     dired-directory dired-marked dired-warning
     flymake-error flymake-warning flymake-note
     ansi-color-red ansi-color-bright-blue)))
"###;
    let expected = expect![[
        r##"OK ((diff-added ((:display #1=((class color) (min-colors 89)) :foreground "#8c9a43" :background "#1d3732" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (diff-removed ((:display #1# :foreground "#d66556" :background "#2d2c31" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (diff-changed ((:display t :foreground nil :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (diff-refine-added ((:display #1# :foreground "#97a35f" :background "#2f4321" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (diff-refine-removed ((:display #1# :foreground "#ce7667" :background "#532725" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (org-level-1 ((:display #1# :foreground "#cb4b16" :background nil :inherit variable-pitch :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height 1.3 :family nil :box nil))) (org-level-2 ((:display #1# :foreground "#859900" :background nil :inherit variable-pitch :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height 1.2 :family nil :box nil))) (org-todo ((:display #1# :foreground "#2aa198" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (org-done ((:display #1# :foreground "#859900" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (org-block ((:display #1# :foreground nil :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (isearch ((:display #1# :foreground "#002b36" :background "#d33682" :inherit nil :weight normal :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (lazy-highlight ((:display #1# :foreground "#002b36" :background "#b58900" :inherit nil :weight normal :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (match ((:display #1# :foreground "#93a1a1" :background "#073642" :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (completions-common-part ((:display t :foreground "#268bd2" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (completions-annotations ((:display t :foreground "#586e75" :background nil :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (dired-directory ((:display #1# :foreground "#268bd2" :background nil :inherit nil :weight normal :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (dired-marked ((:display #1# :foreground "#d33682" :background nil :inherit nil :weight bold :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (dired-warning ((:display #1# :foreground "#cb4b16" :background nil :inherit nil :weight nil :slant nil :underline t :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (flymake-error ((:display ((supports :underline (:style wave)) . #1#) :foreground nil :background nil :inherit unspecified :weight nil :slant nil :underline (:style wave :color "#dc322f") :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil) (:display #1# :foreground "#ff6849" :background "#a7020a" :inherit nil :weight bold :slant nil :underline t :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (flymake-warning ((:display ((supports :underline (:style wave)) . #1#) :foreground nil :background nil :inherit unspecified :weight nil :slant nil :underline (:style wave :color "#b58900") :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil) (:display #1# :foreground "#e1af4b" :background "#866300" :inherit nil :weight bold :slant nil :underline t :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (flymake-note ((:display ((supports :underline (:style wave)) . #1#) :foreground nil :background nil :inherit unspecified :weight nil :slant nil :underline (:style wave :color "#268bd2") :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil) (:display #1# :foreground "#74adf5" :background "#0061a8" :inherit nil :weight bold :slant nil :underline t :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (ansi-color-red ((:display #1# :foreground "#dc322f" :background "#dc322f" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))) (ansi-color-bright-blue ((:display #1# :foreground "#74adf5" :background "#74adf5" :inherit nil :weight nil :slant nil :underline nil :overline nil :extend nil :inverse-video nil :height nil :family nil :box nil))))"##
    ]];
    ParityBatchCase::value(
        "practical_diff_org_search_completion_and_diagnostic_faces_share_the_palette",
        elisp_form,
        expected,
    )
}

fn load_enable_precedence_reenable_and_disable_run_hooks_in_exact_order() -> ParityBatchCase {
    let elisp_form = r###"
(let ((original-themes custom-enabled-themes)
      events states)
  (unwind-protect
      (let ((enable-theme-functions
             (list (lambda (theme) (push (list :enable theme) events))))
            (disable-theme-functions
             (list (lambda (theme) (push (list :disable theme) events)))))
        (dolist (theme '(solarized-dark solarized-light))
          (when (custom-theme-enabled-p theme) (disable-theme theme))
          (neomacs-solarized-test-load theme))
        (push (list :loaded (copy-sequence custom-enabled-themes)
                    :dark (and (custom-theme-p 'solarized-dark) t)
                    :light (and (custom-theme-p 'solarized-light) t))
              states)
        (enable-theme 'solarized-dark)
        (push (list :dark (copy-sequence custom-enabled-themes)) states)
        (enable-theme 'solarized-light)
        (push (list :light-over-dark (copy-sequence custom-enabled-themes)) states)
        (enable-theme 'solarized-dark)
        (push (list :dark-reenabled (copy-sequence custom-enabled-themes)) states)
        (disable-theme 'solarized-light)
        (push (list :light-disabled (copy-sequence custom-enabled-themes)) states)
        (disable-theme 'solarized-dark)
        (push (list :all-disabled (copy-sequence custom-enabled-themes)) states)
        (list :states (nreverse states) :events (nreverse events)))
    (dolist (theme '(solarized-dark solarized-light))
      (when (custom-theme-enabled-p theme) (disable-theme theme)))
    (dolist (theme (reverse original-themes))
      (when (custom-theme-p theme) (enable-theme theme)))))
"###;
    let expected = expect![
        "OK (:states ((:loaded nil :dark t :light t) (:dark (solarized-dark)) (:light-over-dark (solarized-light solarized-dark)) (:dark-reenabled (solarized-dark solarized-light)) (:light-disabled (solarized-dark)) (:all-disabled nil)) :events ((:enable user) (:enable solarized-dark) (:enable user) (:enable solarized-light) (:enable user) (:enable solarized-dark) (:disable solarized-light) (:disable solarized-dark)))"
    ];
    ParityBatchCase::value(
        "load_enable_precedence_reenable_and_disable_run_hooks_in_exact_order",
        elisp_form,
        expected,
    )
}

#[test]
fn solarized_theme_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(SOLARIZED_THEME_MELPA_PIN, "solarized-theme.el")
            .expect("prepare revision-pinned Solarized Theme below ./tmp")
            .with_timeout(Duration::from_secs(300))
            .with_prelude(PRELUDE),
        "solarized-theme-package-batch",
        "Solarized Theme",
        &[
            package_contract_exposes_all_variants_palette_tools_and_policy_defaults(),
            palette_data_and_lab_blending_produce_exact_reusable_color_systems(),
            dark_and_light_themes_resolve_core_editor_and_programming_face_specs(),
            user_options_materially_change_fringe_docs_numbers_typography_and_mode_line(),
            every_shipped_variant_preserves_its_distinct_background_accent_and_comment_palette(),
            practical_diff_org_search_completion_and_diagnostic_faces_share_the_palette(),
            load_enable_precedence_reenable_and_disable_run_hooks_in_exact_order(),
        ],
    );
}
