use expect_test::expect;

use super::ParityBatchCase;

fn catalog_and_policy_defaults_are_registered() -> ParityBatchCase {
    ParityBatchCase::value(
        "catalog_and_policy_defaults_are_registered",
        r####"
(list :items modus-themes-items
      :toggle modus-themes-to-toggle
      :disable-other modus-themes-disable-other-themes
      :italic modus-themes-italic-constructs
      :bold modus-themes-bold-constructs
      :variable-pitch modus-themes-variable-pitch-ui
      :mixed-fonts modus-themes-mixed-fonts
      :known-operandi (and (modus-themes-known-p 'modus-operandi) t)
      :known-vivendi (and (modus-themes-known-p 'modus-vivendi) t)
      :unknown (modus-themes-known-p 'not-a-modus-theme))
"####,
        expect![
            "OK (:items (modus-operandi modus-operandi-tinted modus-operandi-deuteranopia modus-operandi-tritanopia modus-vivendi modus-vivendi-tinted modus-vivendi-deuteranopia modus-vivendi-tritanopia) :toggle (modus-operandi modus-vivendi) :disable-other t :italic nil :bold nil :variable-pitch nil :mixed-fonts nil :known-operandi t :known-vivendi t :unknown nil)"
        ],
    )
}

fn load_theme_switches_between_light_and_dark_and_exposes_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "load_theme_switches_between_light_and_dark_and_exposes_palette",
        r####"
(unwind-protect
    (progn
      (neomacs-modus-themes-test-disable)
      (let ((hook-ran nil))
        (let ((modus-themes-after-load-theme-hook
               (list (lambda () (setq hook-ran t)))))
          (modus-themes-load-theme 'modus-operandi)
          (let* ((current (modus-themes-get-current-theme))
                 (props (get current 'theme-properties))
                 (bg (plist-get props :background-mode))
                 (kind (plist-get props :kind))
                 (family (plist-get props :family))
                 (palette (modus-themes-get-theme-palette current))
                 (bg-value (modus-themes-get-color-value 'bg-main nil current))
                 (fg-value (modus-themes-get-color-value 'fg-main nil current))
                 (red-value (modus-themes-get-color-value 'red nil current)))
            (modus-themes-load-theme 'modus-vivendi)
            (let* ((dark (modus-themes-get-current-theme))
                   (dark-props (get dark 'theme-properties))
                   (dark-bg (plist-get dark-props :background-mode))
                   (dark-bg-value
                    (modus-themes-get-color-value 'bg-main nil dark))
                   (dark-fg-value
                    (modus-themes-get-color-value 'fg-main nil dark)))
              (list :light current
                    :light-mode bg
                    :kind kind
                    :family family
                    :palette-p (and (consp palette) t)
                    :bg-main bg-value
                    :fg-main fg-value
                    :red red-value
                    :hook-ran hook-ran
                    :dark dark
                    :dark-mode dark-bg
                    :dark-bg-main dark-bg-value
                    :dark-fg-main dark-fg-value
                    :enabled (copy-sequence custom-enabled-themes)))))))
  (neomacs-modus-themes-test-disable))
"####,
        expect![[
            r##"OK (:light modus-operandi :light-mode light :kind color-scheme :family modus-themes :palette-p t :bg-main "#ffffff" :fg-main "#000000" :red "#a60000" :hook-ran t :dark modus-vivendi :dark-mode dark :dark-bg-main "#000000" :dark-fg-main "#ffffff" :enabled (modus-vivendi))"##
        ]],
    )
}

fn toggle_and_rotate_cycle_configured_themes() -> ParityBatchCase {
    ParityBatchCase::value(
        "toggle_and_rotate_cycle_configured_themes",
        r####"
(unwind-protect
    (progn
      (neomacs-modus-themes-test-disable)
      (let ((modus-themes-to-toggle '(modus-operandi modus-vivendi))
            (modus-themes-to-rotate
             '(modus-operandi modus-vivendi modus-operandi-tinted)))
        (modus-themes-load-theme 'modus-operandi)
        (let ((after-load (modus-themes-get-current-theme)))
          (modus-themes-toggle)
          (let ((after-toggle (modus-themes-get-current-theme)))
            (modus-themes-toggle)
            (let ((after-toggle-back (modus-themes-get-current-theme)))
              (modus-themes-rotate modus-themes-to-rotate)
              (let ((after-rotate (modus-themes-get-current-theme)))
                (modus-themes-rotate modus-themes-to-rotate t)
                (list :after-load after-load
                      :after-toggle after-toggle
                      :after-toggle-back after-toggle-back
                      :after-rotate after-rotate
                      :after-rotate-reverse
                      (modus-themes-get-current-theme))))))))
  (neomacs-modus-themes-test-disable))
"####,
        expect![
            "OK (:after-load modus-operandi :after-toggle modus-vivendi :after-toggle-back modus-operandi :after-rotate modus-vivendi :after-rotate-reverse modus-operandi)"
        ],
    )
}

fn contrast_formula_and_background_sort_are_deterministic() -> ParityBatchCase {
    ParityBatchCase::value(
        "contrast_formula_and_background_sort_are_deterministic",
        r####"
(dolist (theme '(modus-operandi modus-operandi-tinted
                 modus-vivendi modus-vivendi-tinted))
  (modus-themes-activate theme))
(let* ((black-white (modus-themes-contrast "#000000" "#ffffff"))
       (same (modus-themes-contrast "#abcdef" "#abcdef"))
       (unsorted '(modus-vivendi modus-operandi modus-vivendi-tinted
                   modus-operandi-tinted))
       (sorted-light (modus-themes-sort (copy-sequence unsorted) 'light))
       (sorted-dark (modus-themes-sort (copy-sequence unsorted) 'dark))
       (filtered
        (modus-themes-filter-by-background-mode
         unsorted 'dark)))
  (list :black-white black-white
        :same same
        :sorted-light sorted-light
        :sorted-dark sorted-dark
        :filtered-dark filtered
        :dark-p (modus-themes-color-dark-p "#111111")
        :light-p (modus-themes-color-dark-p "#eeeeee")))
"####,
        expect![
            "OK (:black-white 21.0 :same 1.0 :sorted-light (modus-operandi modus-operandi-tinted modus-vivendi modus-vivendi-tinted) :sorted-dark (modus-vivendi modus-vivendi-tinted modus-operandi modus-operandi-tinted) :filtered-dark (modus-vivendi modus-vivendi-tinted) :dark-p t :light-p nil)"
        ],
    )
}

fn every_official_theme_loads_as_a_color_scheme() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_official_theme_loads_as_a_color_scheme",
        r####"
(let (loaded failures)
  (dolist (theme modus-themes-items)
    (condition-case condition
        (progn
          (load-theme theme t t)
          (let ((props (get theme 'theme-properties)))
            (push
             (list theme
                   (custom-theme-p theme)
                   (plist-get props :kind)
                   (plist-get props :background-mode)
                   (plist-get props :family))
             loaded)))
      (error
       (push (list theme (car condition) (error-message-string condition))
             failures))))
  (list :count (length modus-themes-items)
        :loaded-count (length loaded)
        :themes (nreverse (mapcar #'car loaded))
        :all-color-schemes
        (cl-every (lambda (entry) (eq (nth 2 entry) 'color-scheme)) loaded)
        :background-modes
        (delete-dups
         (sort (mapcar (lambda (entry) (nth 3 entry)) loaded)
               (lambda (left right)
                 (string-lessp (symbol-name left) (symbol-name right)))))
        :families
        (delete-dups
         (sort (mapcar (lambda (entry) (nth 4 entry)) loaded)
               (lambda (left right)
                 (string-lessp (symbol-name left) (symbol-name right)))))
        :failures (nreverse failures)))
"####,
        expect![
            "OK (:count 8 :loaded-count 8 :themes (modus-operandi modus-operandi-tinted modus-operandi-deuteranopia modus-operandi-tritanopia modus-vivendi modus-vivendi-tinted modus-vivendi-deuteranopia modus-vivendi-tritanopia) :all-color-schemes t :background-modes (dark light) :families (modus-themes) :failures nil)"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        catalog_and_policy_defaults_are_registered(),
        load_theme_switches_between_light_and_dark_and_exposes_palette(),
        toggle_and_rotate_cycle_configured_themes(),
        contrast_formula_and_background_sort_are_deterministic(),
        every_official_theme_loads_as_a_color_scheme(),
    ]
}
