use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DOOM_THEMES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'doom-themes)

(defun neomacs-doom-test-disable-themes ()
  "Disable every enabled Doom theme without disturbing unrelated themes."
  (dolist (theme (copy-sequence custom-enabled-themes))
    (when (string-prefix-p "doom-" (symbol-name theme))
      (disable-theme theme))))

(defun neomacs-doom-test-theme-face-attrs (theme face)
  "Return FACE's highest-color attribute plist registered by THEME."
  (let ((entry (assq theme (get face 'theme-face))))
    (and entry (cadr (car (cadr entry))))))

(defun neomacs-doom-test-token (text)
  "Describe TEXT's font-lock face and active theme attributes at point."
  (goto-char (point-min))
  (search-forward text)
  (let* ((position (match-beginning 0))
         (face (get-text-property position 'face)))
    (list text position face
          (and (symbolp face)
               (neomacs-doom-test-theme-face-attrs 'doom-one face)))))
"####;

fn every_shipped_theme_loads_as_a_complete_color_scheme() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((themes
        (sort
         (cl-remove-if-not
          (lambda (theme) (string-prefix-p "doom-" (symbol-name theme)))
          (custom-available-themes))
         (lambda (left right)
           (string-lessp (symbol-name left) (symbol-name right)))))
       loaded
       failures)
  (dolist (theme themes)
    (condition-case condition
        (progn
          (load-theme theme t t)
          (push
           (list theme
                 (custom-theme-p theme)
                 (plist-get (get theme 'theme-properties) :kind)
                 (plist-get (get theme 'theme-properties) :background-mode))
           loaded))
      (error
       (push (list theme (car condition) (error-message-string condition))
             failures))))
  (list :count (length themes)
        :themes themes
        :loaded-count (length loaded)
        :all-color-schemes
        (cl-every (lambda (entry) (eq (nth 2 entry) 'color-scheme)) loaded)
        :background-modes
        (delete-dups (sort (mapcar #'cadddr loaded)
                           (lambda (left right)
                             (string-lessp (symbol-name left)
                                           (symbol-name right)))))
        :failures (nreverse failures)))
"####;
    let expected = expect![
        "OK (:count 77 :themes (doom-1337 doom-Iosvkem doom-acario-dark doom-acario-light doom-ayu-dark doom-ayu-light doom-ayu-mirage doom-badger doom-bluloco-dark doom-bluloco-light doom-challenger-deep doom-city-lights doom-dark+ doom-dracula doom-earl-grey doom-ephemeral doom-fairy-floss doom-feather-dark doom-feather-light doom-flatwhite doom-gruvbox doom-gruvbox-light doom-henna doom-homage-black doom-homage-white doom-horizon doom-ir-black doom-lantern doom-laserwave doom-manegarm doom-material doom-material-dark doom-meltbus doom-miramare doom-molokai doom-monokai-classic doom-monokai-machine doom-monokai-octagon doom-monokai-pro doom-monokai-ristretto doom-monokai-spectrum doom-moonlight doom-nord doom-nord-aurora doom-nord-light doom-nova doom-oceanic-next doom-oksolar-dark doom-oksolar-light doom-old-hope doom-one doom-one-light doom-opera doom-opera-light doom-outrun-electric doom-palenight doom-peacock doom-pine doom-plain doom-plain-dark doom-rouge doom-shades-of-purple doom-snazzy doom-solarized-dark doom-solarized-dark-high-contrast doom-solarized-light doom-sourcerer doom-spacegrey doom-tokyo-night doom-tomorrow-day doom-tomorrow-night doom-vibrant doom-wilmersdorf doom-winter-is-coming-dark-blue doom-winter-is-coming-light doom-xcode doom-zenburn) :loaded-count 77 :all-color-schemes t :background-modes (dark light nil) :failures nil)"
    ];
    ParityBatchCase::value(
        "every_shipped_theme_loads_as_a_complete_color_scheme",
        elisp_form,
        expected,
    )
}

fn switches_between_dark_and_light_release_workspaces() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (progn
      (neomacs-doom-test-disable-themes)
      (load-theme 'doom-one t)
      (let ((dark
             (list
              :enabled (copy-sequence custom-enabled-themes)
              :family (plist-get (get 'doom-one 'theme-properties) :family)
              :mode (plist-get (get 'doom-one 'theme-properties) :background-mode)
              :palette (mapcar (lambda (color)
                                 (list color (doom-color color)
                                       (doom-color color 256)
                                       (doom-color color 16)))
                               '(bg fg keywords strings error success))
              :default (neomacs-doom-test-theme-face-attrs 'doom-one 'default)
              :keyword
              (neomacs-doom-test-theme-face-attrs
               'doom-one 'font-lock-keyword-face))))
        (load-theme 'doom-one-light t)
        (let ((light
               (list
                :enabled (copy-sequence custom-enabled-themes)
                :family
                (plist-get (get 'doom-one-light 'theme-properties) :family)
                :mode
                (plist-get
                 (get 'doom-one-light 'theme-properties) :background-mode)
                :palette (mapcar (lambda (color)
                                   (list color (doom-color color)
                                         (doom-color color 256)
                                         (doom-color color 16)))
                                 '(bg fg keywords strings error success))
                :default
                (neomacs-doom-test-theme-face-attrs
                 'doom-one-light 'default)
                :keyword
                (neomacs-doom-test-theme-face-attrs
                 'doom-one-light 'font-lock-keyword-face))))
          (disable-theme 'doom-one-light)
          (list :dark dark
                :light light
                :after-light-disabled (copy-sequence custom-enabled-themes)
                :dark-still-enabled (custom-theme-enabled-p 'doom-one)))))
  (neomacs-doom-test-disable-themes))
"####;
    let expected = expect![[
        r##"OK (:dark (:enabled (doom-one) :family doom-one :mode dark :palette ((bg "#282c34" "black" "black") (fg "#bbc2cf" "#bfbfbf" "brightwhite") (keywords "#51afef" "#51afef" "brightblue") (strings "#98be65" "#99bb66" "green") (error "#ff6c6b" "#ff6655" "red") (success "#98be65" "#99bb66" "green")) :default (:background "#282c34" :foreground "#bbc2cf") :keyword (:foreground "#51afef")) :light (:enabled (doom-one-light doom-one) :family doom-one :mode light :palette ((bg "#fafafa" "white" "white") (fg "#383a42" "#424242" "black") (keywords "#e45649" "#e45649" "red") (strings "#50a14f" "#50a14f" "green") (error "#e45649" "#e45649" "red") (success "#50a14f" "#50a14f" "green")) :default (:background "#fafafa" :foreground "#383a42") :keyword (:foreground "#e45649")) :after-light-disabled (doom-one) :dark-still-enabled (doom-one))"##
    ]];
    ParityBatchCase::value(
        "switches_between_dark_and_light_release_workspaces",
        elisp_form,
        expected,
    )
}

fn fontifies_real_elisp_with_the_active_theme_face_contracts() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (progn
      (neomacs-doom-test-disable-themes)
      (load-theme 'doom-one t)
      (with-temp-buffer
        (insert
         ";; Publish a release artifact.\n"
         "(defun publish-release (name)\n"
         "  (let ((target \"linux-x86_64\")\n"
         "        (retries 3))\n"
         "    (when (> retries 0)\n"
         "      (message \"shipping %s to %s\" name target))))\n")
        (emacs-lisp-mode)
        (font-lock-ensure (point-min) (point-max))
        (list
         :text (buffer-substring-no-properties (point-min) (point-max))
         :tokens
         (mapcar #'neomacs-doom-test-token
                 '("Publish" "defun" "publish-release" "name" "\"linux-x86_64\""
                   "3" "when" "message" "\"shipping %s to %s\""))
         :modified (buffer-modified-p))))
  (neomacs-doom-test-disable-themes))
"####;
    let expected = expect![[
        r##"OK (:text ";; Publish a release artifact.\n(defun publish-release (name)\n  (let ((target \"linux-x86_64\")\n        (retries 3))\n    (when (> retries 0)\n      (message \"shipping %s to %s\" name target))))\n" :tokens (("Publish" 4 font-lock-comment-face (:foreground "#5B6268" :background unspecified)) ("defun" 33 font-lock-keyword-face #1=(:foreground "#51afef")) ("publish-release" 39 font-lock-function-name-face (:foreground "#c678dd")) ("name" 56 nil nil) ("\"linux-x86_64\"" 78 font-lock-string-face #2=(:foreground "#98be65")) ("3" 111 nil nil) ("when" 120 font-lock-keyword-face #1#) ("message" 146 nil nil) ("\"shipping %s to %s\"" 154 font-lock-string-face #2#)) :modified t)"##
    ]];
    ParityBatchCase::value(
        "fontifies_real_elisp_with_the_active_theme_face_contracts",
        elisp_form,
        expected,
    )
}

fn disabling_bold_and_italic_rewrites_theme_typography_policy() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (let ((doom-themes-enable-bold nil)
          (doom-themes-enable-italic nil))
      (neomacs-doom-test-disable-themes)
      (load-theme 'doom-one t)
      (list
       :enabled custom-enabled-themes
       :faces
       (mapcar
        (lambda (face)
          (let ((attrs
                 (neomacs-doom-test-theme-face-attrs 'doom-one face)))
            (list face
                  :weight (plist-get attrs :weight)
                  :bold (plist-get attrs :bold)
                  :slant (plist-get attrs :slant)
                  :italic (plist-get attrs :italic)
                  :foreground (plist-get attrs :foreground))))
        '(bold
          italic
          font-lock-function-call-face
          font-lock-property-name-face
          font-latex-verbatim-face
          mode-line-buffer-id))))
  (neomacs-doom-test-disable-themes))
"####;
    let expected = expect![[
        r##"OK (:enabled (doom-one) :faces ((bold :weight normal :bold nil :slant nil :italic nil :foreground "#DFDFDF") (italic :weight nil :bold nil :slant normal :italic nil :foreground nil) (font-lock-function-call-face :weight nil :bold nil :slant normal :italic nil :foreground "#c28ed8") (font-lock-property-name-face :weight normal :bold nil :slant nil :italic nil :foreground "#7bb6e2") (font-latex-verbatim-face :weight nil :bold nil :slant normal :italic nil :foreground "#a9a1e1") (mode-line-buffer-id :weight normal :bold nil :slant nil :italic nil :foreground nil)))"##
    ]];
    ParityBatchCase::value(
        "disabling_bold_and_italic_rewrites_theme_typography_policy",
        elisp_form,
        expected,
    )
}

fn composes_application_faces_with_inherit_extend_and_override() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((doom-themes--colors
        '((bg "#101820" "black" "black")
          (bg-alt "#182430" "#1c1c1c" "brightblack")
          (fg "#f2f2f2" "white" "white")
          (blue "#4da6ff" "#5fafff" "brightblue")
          (orange "#ff9f43" "#ffaf5f" "brightred")))
       (bg (cdr (assq 'bg doom-themes--colors)))
       (bg-alt (cdr (assq 'bg-alt doom-themes--colors)))
       (fg (cdr (assq 'fg doom-themes--colors)))
       (blue (cdr (assq 'blue doom-themes--colors)))
       (orange (cdr (assq 'orange doom-themes--colors)))
       (defaults
        '((release-panel :background bg :foreground fg :weight 'normal)
          (release-title :foreground blue :weight 'bold)
          (release-muted :foreground orange :slant 'italic)))
       (custom
        '(((release-panel &override)
           :background bg-alt :box `(:line-width 1 :color ,blue))
          ((release-title &extend) :underline t)
          ((release-caption &inherit release-muted) :weight 'bold)
          (release-button :background blue :foreground bg :weight 'bold)))
       (merged (doom-themes--apply-faces custom defaults))
       (ordered
        (sort merged (lambda (left right)
                       (string-lessp (symbol-name (car left))
                                     (symbol-name (car right)))))))
  (list
   :merged ordered
   :built
   (mapcar
    (lambda (face)
      (eval (doom-themes--build-face face)))
    ordered)
   :source-defaults defaults
   :source-custom custom))
"####;
    let expected = expect![[
        r##"OK (:merged ((release-button . #8=(:background blue :foreground bg :weight 'bold)) (release-caption . #1=(:foreground orange :slant #5='italic :weight #7='bold)) (release-muted . #1#) (release-panel :background bg-alt :foreground fg :weight #3='normal :box #6=`(:line-width 1 :color ,blue)) (release-title :foreground blue :weight #4='bold :underline t)) :built ((release-button (((#2=(class color) (min-colors 257)) (:background "#4da6ff" :foreground "#101820" :weight bold)) ((#2# (min-colors 256)) (:background "#5fafff" :foreground "black" :weight bold)) ((#2# (min-colors 16)) (:background "brightblue" :foreground "black" :weight bold)))) (release-caption (((#2# (min-colors 257)) (:foreground "#ff9f43" :slant italic :weight bold)) ((#2# (min-colors 256)) (:foreground "#ffaf5f" :slant italic :weight bold)) ((#2# (min-colors 16)) (:foreground "brightred" :slant italic :weight bold)))) (release-muted (((#2# (min-colors 257)) (:foreground "#ff9f43" :slant italic :weight bold)) ((#2# (min-colors 256)) (:foreground "#ffaf5f" :slant italic :weight bold)) ((#2# (min-colors 16)) (:foreground "brightred" :slant italic :weight bold)))) (release-panel (((#2# (min-colors 257)) (:background "#182430" :foreground "#f2f2f2" :weight normal :box (:line-width 1 :color "#4da6ff"))) ((#2# (min-colors 256)) (:background "#1c1c1c" :foreground "white" :weight normal :box (:line-width 1 :color "#5fafff"))) ((#2# (min-colors 16)) (:background "brightblack" :foreground "white" :weight normal :box (:line-width 1 :color "brightblue"))))) (release-title (((#2# (min-colors 257)) (:foreground "#4da6ff" :weight bold :underline t)) ((#2# (min-colors 256)) (:foreground "#5fafff" :weight bold :underline t)) ((#2# (min-colors 16)) (:foreground "brightblue" :weight bold :underline t))))) :source-defaults ((release-panel :background bg :foreground fg :weight #3#) (release-title :foreground blue :weight #4#) (release-muted :foreground orange :slant #5#)) :source-custom (((release-panel &override) :background bg-alt :box #6#) ((release-title &extend) :underline t) ((release-caption &inherit release-muted) :weight #7#) (release-button . #8#)))"##
    ]];
    ParityBatchCase::value(
        "composes_application_faces_with_inherit_extend_and_override",
        elisp_form,
        expected,
    )
}

fn derives_accessible_release_status_colors_from_theme_tokens() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (progn
      (neomacs-doom-test-disable-themes)
      (load-theme 'doom-one t)
      (list
       :status-colors
       (mapcar
        (lambda (entry)
          (let ((name (car entry)) (color (cadr entry)))
            (list name color
                  :subtle (doom-blend color (doom-color 'bg) 0.18)
                  :hover (doom-lighten color 0.12)
                  :pressed (doom-darken color 0.20))))
        `((success ,(doom-color 'success))
          (warning ,(doom-color 'warning))
          (failure ,(doom-color 'error))))
       :terminal-levels
       (mapcar (lambda (name)
                 (list name (doom-color name) (doom-color name 256)
                       (doom-color name 16)))
               '(success warning error))
       :alpha-boundaries
       (list (doom-blend (doom-color 'blue) (doom-color 'bg) 0)
             (doom-blend (doom-color 'blue) (doom-color 'bg) 1))))
  (neomacs-doom-test-disable-themes))
"####;
    let expected = expect![[
        r##"OK (:status-colors ((success "#98be65" :subtle "#3c463c" :hover "#a4c577" :pressed "#799850") (warning "#ECBE7B" :subtle "#4b4640" :hover "#eec58a" :pressed "#bc9862") (failure "#ff6c6b" :subtle "#4e373d" :hover "#ff7d7c" :pressed "#cc5655")) :terminal-levels ((success "#98be65" "#99bb66" "green") (warning "#ECBE7B" "#ECBE7B" "yellow") (error "#ff6c6b" "#ff6655" "red")) :alpha-boundaries ("#282c34" "#51afef"))"##
    ]];
    ParityBatchCase::value(
        "derives_accessible_release_status_colors_from_theme_tokens",
        elisp_form,
        expected,
    )
}

fn org_extension_styles_workflow_tags_but_excludes_code_and_links() -> ParityBatchCase {
    let elisp_form = r####"
(require 'org)
(require 'doom-themes-ext-org)
(with-temp-buffer
  (insert
   "* TODO Ship #release @ops\n"
   "- [X] Published #done @release\n"
   "#+begin_src emacs-lisp\n"
   "(message \"#not-a-tag @not-an-owner\")\n"
   "#+end_src\n"
   "See [[https://example.test/#fragment][#linked @label]].\n")
  (org-mode)
  (font-lock-ensure (point-min) (point-max))
  (list
   :text (buffer-substring-no-properties (point-min) (point-max))
   :spans
   (mapcar
    (lambda (needle)
      (goto-char (point-min))
      (search-forward needle)
      (let ((position (match-beginning 0)))
        (goto-char position)
        (list needle position
              (get-text-property position 'face)
              (org-element-type (org-element-context)))))
    '("#release" "@ops" "[X]" "#done" "@release"
      "#not-a-tag" "@not-an-owner" "#linked" "@label"))
   :keyword-hook-installed
   (memq #'doom-themes-enable-org-fontification
         org-font-lock-set-keywords-hook)))
"####;
    let expected = expect![[
        r##"OK (:text "* TODO Ship #release @ops\n- [X] Published #done @release\n#+begin_src emacs-lisp\n(message \"#not-a-tag @not-an-owner\")\n#+end_src\nSee [[https://example.test/#fragment][#linked @label]].\n" :spans (("#release" 13 (doom-themes-org-hash-tag org-level-1) headline) ("@ops" 22 (doom-themes-org-at-tag org-level-1) headline) ("[X]" 29 (org-headline-done org-checkbox) item) ("#done" 43 (doom-themes-org-hash-tag . #1=(org-headline-done)) paragraph) ("@release" 49 (doom-themes-org-at-tag . #1#) paragraph) ("#not-a-tag" 91 #2=(font-lock-string-face org-block) src-block) ("@not-an-owner" 102 #2# src-block) ("#linked" 166 org-link link) ("@label" 174 org-link link)) :keyword-hook-installed (doom-themes-enable-org-fontification))"##
    ]];
    ParityBatchCase::value(
        "org_extension_styles_workflow_tags_but_excludes_code_and_links",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn doom_themes_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DOOM_THEMES_MELPA_PIN, "doom-themes.el")
        .expect("prepare pinned Doom Themes source below ./tmp")
        .with_timeout(Duration::from_secs(300))
        .with_prelude(PRELUDE)
}

#[test]
fn doom_themes_practical_workflows_batch() {
    let cases = vec![
        every_shipped_theme_loads_as_a_complete_color_scheme(),
        switches_between_dark_and_light_release_workspaces(),
        fontifies_real_elisp_with_the_active_theme_face_contracts(),
        disabling_bold_and_italic_rewrites_theme_typography_policy(),
        composes_application_faces_with_inherit_extend_and_override(),
        derives_accessible_release_status_colors_from_theme_tokens(),
        org_extension_styles_workflow_tags_but_excludes_code_and_links(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("doom-themes parity batch");
    assert_oracle_batch_cases(
        doom_themes_oracle(),
        test_name,
        "doom-themes parity",
        &cases,
    );
}
