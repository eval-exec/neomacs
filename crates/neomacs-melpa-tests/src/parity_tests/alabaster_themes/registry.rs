use expect_test::expect;

use super::ParityBatchCase;

fn exact_release_public_collections_commands_and_customization_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "exact_release_public_collections_commands_and_customization_surface",
        r##"
(let* ((descriptor
        (cadr (assq 'alabaster-themes package-alist)))
       (extras (package-desc-extras descriptor)))
  (list
   (package-desc-name descriptor)
   (package-version-join (package-desc-version descriptor))
   (alist-get :commit extras)
   (alist-get :url extras)
   (featurep 'alabaster-themes)
   (get 'alabaster-themes 'group-documentation)
   alabaster-themes-light-themes
   alabaster-themes-dark-themes
   alabaster-themes-collection
   (mapcar
    (lambda (command)
      (list command
            (fboundp command)
            (commandp command)))
    '(alabaster-themes-select
      alabaster-themes-list-colors
      alabaster-themes-preview-mode))
   (mapcar
    (lambda (option)
      (list option
            (custom-variable-p option)
            (get option 'custom-type)
            (get option 'custom-group)))
    '(alabaster-themes-post-load-hook
      alabaster-themes-common-palette-overrides
      alabaster-themes-no-bold
      alabaster-themes-headings
      alabaster-themes-mixed-fonts
      alabaster-themes-variable-pitch-ui))))
"##,
        expect![[
            r#"OK (alabaster-themes "20260113.657" "2d3dcfc6ac8988d23f8065a580f7dcf4ff607e56" "https://github.com/vedang/alabaster-themes" t "Minimal Alabaster themes." (alabaster-themes-light alabaster-themes-light-bg alabaster-themes-light-mono) #1=(alabaster-themes-dark alabaster-themes-dark-mono) (alabaster-themes-light alabaster-themes-light-bg alabaster-themes-light-mono . #1#) ((alabaster-themes-select t t) (alabaster-themes-list-colors t t) (alabaster-themes-preview-mode t nil)) ((alabaster-themes-post-load-hook ((funcall #'#[nil (nil) #2=(t)])) hook nil) (alabaster-themes-common-palette-overrides ((funcall #'#[nil (nil) #2#])) (repeat (list symbol (choice symbol string))) nil) (alabaster-themes-no-bold ((funcall #'#[nil (nil) #2#])) boolean nil) (alabaster-themes-headings ((funcall #'#[nil (nil) #4=(alabaster-themes-preview-mode-abbrev-table alabaster-themes-preview-mode-syntax-table . #2#)])) (alist :options ((0 #3=(set :tag "Properties" :greedy t (const :tag "Proportionately spaced font (variable-pitch)" variable-pitch) (choice :tag "Font weight (must be supported by the typeface)" (const :tag "Bold (default)" nil) (const :tag "Thin" thin) (const :tag "Ultra-light" ultralight) (const :tag "Extra-light" extralight) (const :tag "Light" light) (const :tag "Semi-light" semilight) (const :tag "Regular" regular) (const :tag "Medium" medium) (const :tag "Semi-bold" semibold) (const :tag "Extra-bold" extrabold) (const :tag "Ultra-bold" ultrabold)) (radio :tag "Height" (float :tag "Floating point to adjust height by") (cons :tag "Cons cell of `(height . FLOAT)'" (const :tag "The `height' key (constant)" height) (float :tag "Floating point"))))) (1 #3#) (2 #3#) (3 #3#) (4 #3#) (5 #3#) (6 #3#) (7 #3#) (8 #3#) (t #3#) (agenda-date #3#) (agenda-structure #3#)) :key-type symbol :value-type #3#) nil) (alabaster-themes-mixed-fonts ((funcall #'#[nil (nil) #4#])) boolean nil) (alabaster-themes-variable-pitch-ui ((funcall #'#[nil (nil) #4#])) boolean nil)))"#
        ]],
    )
}

fn every_theme_file_loads_without_enabling_and_registers_complete_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_theme_file_loads_without_enabling_and_registers_complete_metadata",
        r##"
(let ((themes alabaster-themes-collection))
  (list
   (mapcar
    (lambda (theme)
      (let ((loaded (load-theme theme t t)))
        (list
         theme
         loaded
         (custom-theme-p theme)
         (memq theme custom-known-themes)
         (get theme 'theme-feature)
         (get theme 'theme-documentation)
         (length (get theme 'theme-settings))
         (seq-count
          (lambda (setting)
            (eq (car setting) 'theme-face))
          (get theme 'theme-settings)))))
    themes)
   custom-enabled-themes
   (mapcar
    (lambda (theme)
      (file-name-nondirectory
       (locate-library (format "%s-theme" theme))))
    themes)))
"##,
        expect![[
            r#"OK (((alabaster-themes-light t #1=(alabaster-themes-light user changed) #1# alabaster-themes-light-theme "Minimal light theme with foreground highlighting." 501 501) (alabaster-themes-light-bg t #2=(alabaster-themes-light-bg . #1#) #2# alabaster-themes-light-bg-theme "Minimal light theme with background highlighting." 103 103) (alabaster-themes-light-mono t #3=(alabaster-themes-light-mono . #2#) #3# alabaster-themes-light-mono-theme "Minimal light theme with monochromatic highlighting." 501 501) (alabaster-themes-dark t #4=(alabaster-themes-dark . #3#) #4# alabaster-themes-dark-theme "Minimal dark theme with foreground highlighting." 501 501) (alabaster-themes-dark-mono t #5=(alabaster-themes-dark-mono . #4#) #5# alabaster-themes-dark-mono-theme "Minimal dark theme with monochromatic highlighting." 501 501)) nil ("alabaster-themes-light-theme.el" "alabaster-themes-light-bg-theme.el" "alabaster-themes-light-mono-theme.el" "alabaster-themes-dark-theme.el" "alabaster-themes-dark-mono-theme.el"))"#
        ]],
    )
}

fn complete_function_signatures_interactive_contracts_and_docs_are_stable() -> ParityBatchCase {
    ParityBatchCase::value(
        "complete_function_signatures_interactive_contracts_and_docs_are_stable",
        r##"
(mapcar
 (lambda (function)
   (list
    function
    (help-function-arglist function t)
    (interactive-form function)
    (secure-hash 'sha256 (documentation function))))
 '(alabaster-themes--retrieve-palette-value
   alabaster-themes--list-enabled-themes
   alabaster-themes--enable-themes
   alabaster-themes--list-known-themes
   alabaster-themes--current-theme
   alabaster-themes--palette-symbol
   alabaster-themes--palette-value
   alabaster-themes--current-theme-palette
   alabaster-themes--annotate-theme
   alabaster-themes--completion-table
   alabaster-themes--load-subset
   alabaster-themes--maybe-prompt-subset
   alabaster-themes--choose-subset
   alabaster-themes--select-prompt
   alabaster-themes--disable-themes
   alabaster-themes-load-theme
   alabaster-themes-select
   alabaster-themes--list-colors-get-mappings
   alabaster-themes--list-colors-tabulated
   alabaster-themes--set-tabulated-entries
   alabaster-themes-list-colors
   alabaster-themes-preview-mode
   alabaster-themes--weight
   alabaster-themes--property-lookup
   alabaster-themes--heading
   alabaster-themes--fixed-pitch
   alabaster-themes--variable-pitch-ui
   alabaster-themes--bold))
"##,
        expect![[
            r#"OK ((alabaster-themes--retrieve-palette-value (color palette) nil "f7fcfda27d761a2ba2bd226b219307edbb4a10c9c0941ca64a404dbe9a32c9f4") (alabaster-themes--list-enabled-themes nil nil "93813b1c50f17ca25de4694cbcf5557c3382a666981733619b23f52781d60bf9") (alabaster-themes--enable-themes (&optional subset) nil "258c665a2b94407ebc027830fc8ee71d0190c44f41b16275ccc29595a7dd0036") (alabaster-themes--list-known-themes nil nil "39cf1bd732b625ac81d83d1541b8e659261fdfc4d7d457ad1b99037bfa0d2dee") (alabaster-themes--current-theme nil nil "9002e4f82f59b5a3e52d2bee6b589897c80f049ab309fa00d0af11d6532f32f3") (alabaster-themes--palette-symbol (theme &optional overrides) nil "8a0a6c2b8259b87289a0f27629fd9efef9258df7aabb6b22387a34e10d8f1bba") (alabaster-themes--palette-value (theme &optional overrides) nil "30b0ab001833e11a839c4b0e9a13075e8ea822019903af12ce380ed648df70be") (alabaster-themes--current-theme-palette (&optional overrides) nil "20c58a293222c6ec2f76e9f60e5f8463c156adb8c475ff434acd21c96afe622f") (alabaster-themes--annotate-theme (theme) nil "ce732b14807fa1280c6f4855da29eeb82ac8a64a33e3a0e0e72cc7e18ebfb1be") (alabaster-themes--completion-table (category candidates) nil "57b772ca46008bcc67cf9d5760008d94f3f454aec1928c8734e9f67980b4f9ac") (alabaster-themes--load-subset (subset) nil "5d91958eaf4f1d7dfe7d10fece46e13280be923e2b817e8b6f30a7f958016587") (alabaster-themes--maybe-prompt-subset (variant) nil "a50e53d359515a9a4a8df3434da6ed1c3aa3f414c9c9f58505e9b495a7370020") (alabaster-themes--choose-subset nil nil "a6c66f3d87c680dd583770eb78307f5f1587c2ac61bf9533c13fe32cfdee1cde") (alabaster-themes--select-prompt (&optional prompt variant) nil "bbccefdcafc9e03f82688dc49312f4c0f9f610a8a93b0c27e3d0ae70fc197ea1") (alabaster-themes--disable-themes nil nil "4306343980752a6ef48c58fa5d4acb14f6ee1569e0d32ed6be7cd3bcd8fc5f4a") (alabaster-themes-load-theme (theme) nil "539afe07edc499e5f03d3118ea3bb5ab16463998eece3a65d584ddd666c6dd36") (alabaster-themes-select (theme &optional _variant) (interactive (list (alabaster-themes--select-prompt nil current-prefix-arg))) "8699ecf0ffa5e304dd2b26a170ccacb0378c404b256a09bdc269977b8ccef14c") (alabaster-themes--list-colors-get-mappings (palette) nil "ffe65f5aa70122bcb6ca2967246b29efba9e2a547f8f8bc771a5d26aefeb15b9") (alabaster-themes--list-colors-tabulated (theme &optional mappings) nil "2a8c4f0af735738ed993ff406771bba848c3974eccc7864e408243051db993eb") (alabaster-themes--set-tabulated-entries nil nil "91b1fb9c281bc33d75f6b77d2f94b4d8dd4237e75b51d653a427ab78b4600df0") (alabaster-themes-list-colors (theme &optional mappings) (interactive (let ((prompt (if current-prefix-arg "Preview palette mappings of THEME: " "Preview palette of THEME: "))) (list (alabaster-themes--select-prompt prompt) current-prefix-arg))) "9c011d90ac9a99926dcd3fe3e1fa3ffbf2f01c4617552bab4a1e055fb1c11a70") (alabaster-themes-preview-mode nil nil "c34bfbb6d8aabc106a3b50b475e612624fa7c42bf09bc8aa5c67896afe882644") (alabaster-themes--weight (list) nil "18f6d99c978f8e32584372504bd53e21b81dd8f6cb86986ef8869342e48adc67") (alabaster-themes--property-lookup (properties alist-key list-pred default) nil "8e546584bcdd7bb088e7bb9463d45ee46e6b87e11df7ae2cd0312ee353e7dd8d") (alabaster-themes--heading (level) nil "9532ac67294d59f68f68b8380ed7ee5ffe4e15fe04611b5bf57f061a40803efb") (alabaster-themes--fixed-pitch nil nil "4cefe5552f52a6e1b07783181b708fdea55cbed1afe210ede0c8168c8c599017") (alabaster-themes--variable-pitch-ui nil nil "d095bb2611f9796bd4decc34382e75f115a92d84cd8ec10917f94d6118fcda45") (alabaster-themes--bold nil nil "18824fc9d17c89920fde3e92cdfcabc6ea747ef3c3ee871c35b3cc7e995a7192"))"#
        ]],
    )
    .fresh_process()
}

fn strengthened_upstream_smoke_contract_covers_all_variants_palettes_and_faces() -> ParityBatchCase
{
    ParityBatchCase::value(
        "strengthened_upstream_smoke_contract_covers_all_variants_palettes_and_faces",
        r##"
(progn
  (mapc
   (lambda (theme)
     (load-theme theme t t))
   alabaster-themes-collection)
  (let ((alabaster-themes-no-bold nil)
        (faces
         '(default mode-line mode-line-inactive
           font-lock-string-face font-lock-constant-face
           font-lock-comment-face font-lock-function-name-face
           error warning success)))
    (list
     (mapcar
      (lambda (theme)
        (let* ((palette-symbol
                (alabaster-themes--palette-symbol theme))
               (palette (symbol-value palette-symbol)))
          (list theme
                (custom-theme-p theme)
                (boundp palette-symbol)
                (length palette)
                (mapcar
                 (lambda (key)
                   (assq key palette))
                 '(bg-main fg-main string comment constant fnname)))))
      alabaster-themes-collection)
     (mapcar
      (lambda (face)
        (list face (facep face)))
      faces)
     (alabaster-themes--bold)
     (alabaster-themes--heading 1)
     (let ((alabaster-themes-no-bold t))
       (list
        (alabaster-themes--bold)
        (alabaster-themes--heading 1))))))
"##,
        expect![[
            r##"OK (((alabaster-themes-light #1=(alabaster-themes-light user changed) t 108 ((bg-main "#F7F7F7") (fg-main "#000000") (string green) (comment red) (constant magenta) (fnname blue))) (alabaster-themes-light-bg #2=(alabaster-themes-light-bg . #1#) t 108 ((bg-main "#ffffff") (fg-main "#000000") (string green) (comment yellow) (constant magenta) (fnname blue))) (alabaster-themes-light-mono #3=(alabaster-themes-light-mono . #2#) t 108 ((bg-main "#F7F7F7") (fg-main "#000000") (string fg-main) (comment fg-dim) (constant fg-main) (fnname fg-main))) (alabaster-themes-dark #4=(alabaster-themes-dark . #3#) t 108 ((bg-main "#0E1415") (fg-main "#CECECE") (string green) (comment red) (constant magenta) (fnname blue))) (alabaster-themes-dark-mono (alabaster-themes-dark-mono . #4#) t 108 ((bg-main "#0E1415") (fg-main "#CECECE") (string fg-main) (comment fg-dim) (constant fg-main) (fnname fg-main)))) ((default [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (mode-line [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (mode-line-inactive [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (font-lock-string-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (font-lock-constant-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (font-lock-comment-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (font-lock-function-name-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (error [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (warning [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]) (success [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified])) (:inherit bold) (:inherit bold :height unspecified :weight unspecified) (nil (:inherit default :height unspecified :weight unspecified)))"##
        ]],
    )
}

fn package_generated_autoloads_register_commands_mode_and_theme_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_generated_autoloads_register_commands_mode_and_theme_directory",
        r##"
(let* ((source-directory
        (file-name-directory
         (getenv "NEOMACS_PACKAGE_SOURCE")))
       ;; Mask the installed package's own directory.  Spelling it out
       ;; pinned the harness's acquisition layout, so this expectation
       ;; broke when the cache moved from package-cache/ to the
       ;; revision-pinned source-install-cache/ -- a harness change
       ;; wearing the shape of a package regression.  What the assertion
       ;; is about is that the installed directory is on
       ;; `custom-theme-load-path'.
       (mask
        (lambda (value)
          (if (stringp value)
              (replace-regexp-in-string
               (regexp-quote source-directory)
               "[PACKAGE]/"
               value t t)
            value))))
  (list
   (mapcar
    (lambda (symbol)
      (let ((definition (symbol-function symbol)))
        (list
         symbol
         (autoloadp definition)
         (nth 1 definition)
         (nth 3 definition)
         (nth 4 definition))))
    '(alabaster-themes-select
      alabaster-themes-list-colors
      alabaster-themes-preview-mode))
   (mapcar mask (member source-directory custom-theme-load-path))
   (mapcar
    (lambda (theme)
      (member theme (custom-available-themes)))
    '(alabaster-themes-light
      alabaster-themes-light-bg
      alabaster-themes-dark
      alabaster-themes-light-mono
      alabaster-themes-dark-mono))))
"##,
        expect![[
            r#"OK (((alabaster-themes-select t "alabaster-themes" t nil) (alabaster-themes-list-colors t "alabaster-themes" t nil) (alabaster-themes-preview-mode t "alabaster-themes" nil nil)) ("[PACKAGE]/" custom-theme-directory t) ((alabaster-themes-light adwaita deeper-blue dichromacy leuven-dark leuven light-blue manoj-dark misterioso modus-operandi-deuteranopia modus-operandi modus-operandi-tinted modus-operandi-tritanopia modus-vivendi-deuteranopia modus-vivendi modus-vivendi-tinted modus-vivendi-tritanopia newcomers-presets tango-dark tango tsdh-dark tsdh-light wheatgrass whiteboard wombat) (alabaster-themes-light-bg alabaster-themes-light-mono alabaster-themes-light adwaita deeper-blue dichromacy leuven-dark leuven light-blue manoj-dark misterioso modus-operandi-deuteranopia modus-operandi modus-operandi-tinted modus-operandi-tritanopia modus-vivendi-deuteranopia modus-vivendi modus-vivendi-tinted modus-vivendi-tritanopia newcomers-presets tango-dark tango tsdh-dark tsdh-light wheatgrass whiteboard wombat) (alabaster-themes-dark alabaster-themes-light-bg alabaster-themes-light-mono alabaster-themes-light adwaita deeper-blue dichromacy leuven-dark leuven light-blue manoj-dark misterioso modus-operandi-deuteranopia modus-operandi modus-operandi-tinted modus-operandi-tritanopia modus-vivendi-deuteranopia modus-vivendi modus-vivendi-tinted modus-vivendi-tritanopia newcomers-presets tango-dark tango tsdh-dark tsdh-light wheatgrass whiteboard wombat) (alabaster-themes-light-mono alabaster-themes-light adwaita deeper-blue dichromacy leuven-dark leuven light-blue manoj-dark misterioso modus-operandi-deuteranopia modus-operandi modus-operandi-tinted modus-operandi-tritanopia modus-vivendi-deuteranopia modus-vivendi modus-vivendi-tinted modus-vivendi-tritanopia newcomers-presets tango-dark tango tsdh-dark tsdh-light wheatgrass whiteboard wombat) (alabaster-themes-dark-mono alabaster-themes-dark alabaster-themes-light-bg alabaster-themes-light-mono alabaster-themes-light adwaita deeper-blue dichromacy leuven-dark leuven light-blue manoj-dark misterioso modus-operandi-deuteranopia modus-operandi modus-operandi-tinted modus-operandi-tritanopia modus-vivendi-deuteranopia modus-vivendi modus-vivendi-tinted modus-vivendi-tritanopia newcomers-presets tango-dark tango tsdh-dark tsdh-light wheatgrass whiteboard wombat)))"#
        ]],
    )
}

pub(super) fn registry_alabaster_themes_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        exact_release_public_collections_commands_and_customization_surface(),
        every_theme_file_loads_without_enabling_and_registers_complete_metadata(),
        complete_function_signatures_interactive_contracts_and_docs_are_stable(),
        strengthened_upstream_smoke_contract_covers_all_variants_palettes_and_faces(),
    ]
}

pub(super) fn registry_alabaster_themes_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![package_generated_autoloads_register_commands_mode_and_theme_directory()]
}
