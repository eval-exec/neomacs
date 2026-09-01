use expect_test::expect;

use super::ParityBatchCase;

fn every_palette_has_complete_unique_ordered_and_fingerprinted_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_palette_has_complete_unique_ordered_and_fingerprinted_entries",
        r##"
(progn
  (mapc
   (lambda (theme)
     (load-theme theme t t))
   alabaster-themes-collection)
  (mapcar
   (lambda (theme)
     (let* ((palette
             (symbol-value
              (alabaster-themes--palette-symbol theme)))
            (keys (mapcar #'car palette)))
       (list
        theme
        (length palette)
        (length (delete-dups (copy-sequence keys)))
        (car palette)
        (car (last palette))
        (secure-hash 'sha256 (prin1-to-string palette))
        (mapcar
         (lambda (key)
           (assq key palette))
         '(bg-main fg-main red green yellow blue magenta
           bg-added fg-added bg-removed fg-removed
           builtin comment constant fnname keyword string type
           fg-term-red bg-term-red)))))
   alabaster-themes-collection))
"##,
        expect![[
            r##"OK ((alabaster-themes-light 108 108 #1=(bg-main "#F7F7F7") (bg-term-white "gray65") "a96a4af3ba35da3e927cc023bc8706df08d41956118932e4e77a07bfdb895469" (#1# (fg-main "#000000") (red "#AA3731") (green "#448C27") (yellow "#FFBC5D") (blue "#325CC0") (magenta "#7A3E9D") (bg-added "#d4f6d4") (fg-added "#005000") (bg-removed "#ffd4d8") (fg-removed "#8f1313") (builtin red) (comment red) (constant magenta) (fnname blue) (keyword fg-main) (string green) (type fg-main) (fg-term-red red) (bg-term-red red))) (alabaster-themes-light-bg 108 108 #2=(bg-main "#ffffff") (bg-term-white "gray65") "2838237560feef7f2e346b5269800f4fd0ef52b63aa3c4d2c416485e0497daec" (#2# (fg-main "#000000") (red "#AA3731") (green "#448C27") (yellow "#FFBC5D") (blue "#325CC0") (magenta "#7A3E9D") (bg-added "#d4f6d4") (fg-added "#005000") (bg-removed "#ffd4d8") (fg-removed "#8f1313") (builtin red) (comment yellow) (constant magenta) (fnname blue) (keyword fg-main) (string green) (type fg-main) (fg-term-red red) (bg-term-red red))) (alabaster-themes-light-mono 108 108 #3=(bg-main "#F7F7F7") (bg-term-white "gray65") "3ac8822fdcf39341b60d98cf056ae1baf5aaf1141e21b66407dc45c1e34acc88" (#3# (fg-main "#000000") (red "#AA3731") (green "#000000") (yellow "#FFBC5D") (blue "#000000") (magenta "#000000") (bg-added "#d4f6d4") (fg-added "#005000") (bg-removed "#ffd4d8") (fg-removed "#8f1313") (builtin fg-main) (comment fg-dim) (constant fg-main) (fnname fg-main) (keyword fg-main) (string fg-main) (type fg-main) (fg-term-red red) (bg-term-red red))) (alabaster-themes-dark 108 108 #4=(bg-main "#0E1415") (bg-term-white "gray65") "5f52873a7aa5b2998c6cd384eaee81b1f816d98aa90808f13452a862b03972ea" (#4# (fg-main "#CECECE") (red "#DFDF8E") (green "#95CB82") (yellow "#CD974B") (blue "#8AB1F0") (magenta "#CC8BC9") (bg-added "#1f3a1f") (fg-added "#95CB82") (bg-removed "#3a1f1f") (fg-removed "#ff6b6b") (builtin red) (comment red) (constant magenta) (fnname blue) (keyword fg-main) (string green) (type fg-main) (fg-term-red red) (bg-term-red red))) (alabaster-themes-dark-mono 108 108 #5=(bg-main "#0E1415") (bg-term-white "gray65") "f976d9ab5489ae72d82bee3bfbdabddca617a8f86381ab767567160dd743bf97" (#5# (fg-main "#CECECE") (red "#ff6b6b") (green "#CECECE") (yellow "#CD974B") (blue "#CECECE") (magenta "#CECECE") (bg-added "#1f3a1f") (fg-added "#95CB82") (bg-removed "#3a1f1f") (fg-removed "#ff6b6b") (builtin fg-main) (comment fg-dim) (constant fg-main) (fnname fg-main) (keyword fg-main) (string fg-main) (type fg-main) (fg-term-red red) (bg-term-red red))))"##
        ]],
    )
}

fn semantic_palette_mappings_recursively_resolve_for_all_five_variants() -> ParityBatchCase {
    ParityBatchCase::value(
        "semantic_palette_mappings_recursively_resolve_for_all_five_variants",
        r##"
(progn
  (mapc
   (lambda (theme)
     (load-theme theme t t))
   alabaster-themes-collection)
  (mapcar
   (lambda (theme)
     (let ((palette
            (alabaster-themes--palette-value theme)))
       (cons
        theme
        (mapcar
         (lambda (key)
           (list
            key
            (car (alist-get key palette))
            (alabaster-themes--retrieve-palette-value
             key palette)))
         '(err warning info link name keybind identifier prompt
           builtin comment constant docstring fnname keyword
           preprocessor string type variable
           bg-fringe fg-fringe)))))
   alabaster-themes-collection))
"##,
        expect![[
            r##"OK ((alabaster-themes-light (err red "#AA3731") (warning yellow "#FFBC5D") (info green "#448C27") (link blue "#325CC0") (name blue "#325CC0") (keybind red "#AA3731") (identifier magenta "#7A3E9D") (prompt blue "#325CC0") (builtin red "#AA3731") (comment red "#AA3731") (constant magenta "#7A3E9D") (docstring green "#448C27") (fnname blue "#325CC0") (keyword fg-main "#000000") (preprocessor blue "#325CC0") (string green "#448C27") (type fg-main "#000000") (variable blue "#325CC0") (bg-fringe unspecified unspecified) (fg-fringe fg-dim "#777777")) (alabaster-themes-light-bg (err red "#AA3731") (warning yellow "#FFBC5D") (info green "#448C27") (link blue "#325CC0") (name blue "#325CC0") (keybind red "#AA3731") (identifier magenta "#7A3E9D") (prompt blue "#325CC0") (builtin red "#AA3731") (comment yellow "#FFBC5D") (constant magenta "#7A3E9D") (docstring green "#448C27") (fnname blue "#325CC0") (keyword fg-main "#000000") (preprocessor blue "#325CC0") (string green "#448C27") (type fg-main "#000000") (variable blue "#325CC0") (bg-fringe unspecified unspecified) (fg-fringe fg-dim "#777777")) (alabaster-themes-light-mono (err red "#AA3731") (warning yellow "#FFBC5D") (info fg-main "#000000") (link fg-main "#000000") (name fg-main "#000000") (keybind red "#AA3731") (identifier fg-main "#000000") (prompt fg-main "#000000") (builtin fg-main "#000000") (comment fg-dim "#777777") (constant fg-main "#000000") (docstring fg-main "#000000") (fnname fg-main "#000000") (keyword fg-main "#000000") (preprocessor fg-main "#000000") (string fg-main "#000000") (type fg-main "#000000") (variable fg-main "#000000") (bg-fringe unspecified unspecified) (fg-fringe fg-dim "#777777")) (alabaster-themes-dark (err red "#DFDF8E") (warning yellow "#CD974B") (info green "#95CB82") (link blue "#8AB1F0") (name blue "#8AB1F0") (keybind red "#DFDF8E") (identifier magenta "#CC8BC9") (prompt blue "#8AB1F0") (builtin red "#DFDF8E") (comment red "#DFDF8E") (constant magenta "#CC8BC9") (docstring green "#95CB82") (fnname blue "#8AB1F0") (keyword fg-main "#CECECE") (preprocessor blue "#8AB1F0") (string green "#95CB82") (type fg-main "#CECECE") (variable blue "#8AB1F0") (bg-fringe unspecified unspecified) (fg-fringe fg-dim "#666666")) (alabaster-themes-dark-mono (err red "#ff6b6b") (warning yellow "#CD974B") (info fg-main "#CECECE") (link fg-main "#CECECE") (name fg-main "#CECECE") (keybind red "#ff6b6b") (identifier fg-main "#CECECE") (prompt fg-main "#CECECE") (builtin fg-main "#CECECE") (comment fg-dim "#666666") (constant fg-main "#CECECE") (docstring fg-main "#CECECE") (fnname fg-main "#CECECE") (keyword fg-main "#CECECE") (preprocessor fg-main "#CECECE") (string fg-main "#CECECE") (type fg-main "#CECECE") (variable fg-main "#CECECE") (bg-fringe unspecified unspecified) (fg-fringe fg-dim "#666666")))"##
        ]],
    )
}

fn terminal_foreground_and_background_slots_resolve_to_practical_ansi_tables() -> ParityBatchCase {
    ParityBatchCase::value(
        "terminal_foreground_and_background_slots_resolve_to_practical_ansi_tables",
        r##"
(progn
  (mapc
   (lambda (theme)
     (load-theme theme t t))
   alabaster-themes-collection)
  (let ((foregrounds
         '(fg-term-black fg-term-red fg-term-green fg-term-yellow
           fg-term-blue fg-term-magenta fg-term-cyan fg-term-white))
        (backgrounds
         '(bg-term-black bg-term-red bg-term-green bg-term-yellow
           bg-term-blue bg-term-magenta bg-term-cyan bg-term-white)))
    (mapcar
     (lambda (theme)
       (let ((palette
              (alabaster-themes--palette-value theme)))
         (list
          theme
          (mapcar
           (lambda (key)
             (alabaster-themes--retrieve-palette-value key palette))
           foregrounds)
          (mapcar
           (lambda (key)
             (alabaster-themes--retrieve-palette-value key palette))
           backgrounds))))
     alabaster-themes-collection)))
"##,
        expect![[
            r##"OK ((alabaster-themes-light ("black" "#AA3731" "#448C27" "#FFBC5D" "#325CC0" "#7A3E9D" "#325CC0" "gray65") ("black" "#AA3731" "#448C27" "#FFBC5D" "#325CC0" "#7A3E9D" "#325CC0" "gray65")) (alabaster-themes-light-bg ("black" "#AA3731" "#448C27" "#FFBC5D" "#325CC0" "#7A3E9D" "#325CC0" "gray65") ("black" "#AA3731" "#448C27" "#FFBC5D" "#325CC0" "#7A3E9D" "#325CC0" "gray65")) (alabaster-themes-light-mono ("black" "#AA3731" "#000000" "#FFBC5D" "#000000" "#000000" "#000000" "gray65") ("black" "#AA3731" "#F7F7F7" "#F7F7F7" "#F7F7F7" "#F7F7F7" "#F7F7F7" "gray65")) (alabaster-themes-dark ("black" "#DFDF8E" "#95CB82" "#CD974B" "#8AB1F0" "#CC8BC9" "#8AB1F0" "gray65") ("black" "#DFDF8E" "#95CB82" "#CD974B" "#8AB1F0" "#CC8BC9" "#8AB1F0" "gray65")) (alabaster-themes-dark-mono ("black" "#ff6b6b" "#CECECE" "#CD974B" "#CECECE" "#CECECE" "#CECECE" "gray65") ("black" "#ff6b6b" "#0E1415" "#0E1415" "#0E1415" "#0E1415" "#0E1415" "gray65")))"##
        ]],
    )
}

fn palette_lookup_handles_literals_nested_aliases_unspecified_and_missing_keys() -> ParityBatchCase
{
    ParityBatchCase::value(
        "palette_lookup_handles_literals_nested_aliases_unspecified_and_missing_keys",
        r##"
(let ((palette
       '((literal "#123456")
         (alias literal)
         (nested alias)
         (explicit unspecified)
         (nil-value nil)
         (number-value 42))))
  (mapcar
   (lambda (key)
     (list
      key
      (alabaster-themes--retrieve-palette-value key palette)))
   '(literal alias nested explicit nil-value
     number-value absent)))
"##,
        expect![[
            r##"OK ((literal "#123456") (alias "#123456") (nested "#123456") (explicit unspecified) (nil-value unspecified) (number-value unspecified) (absent unspecified))"##
        ]],
    )
}

fn per_theme_overrides_precede_common_overrides_without_mutating_base_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "per_theme_overrides_precede_common_overrides_without_mutating_base_palette",
        r##"
(progn
  (load-theme 'alabaster-themes-light t t)
  (let ((alabaster-themes-common-palette-overrides
         '((blue "#COMMON-BLUE")
           (green "#COMMON-GREEN")
           (new-common "#COMMON-ONLY")))
        (alabaster-themes-light-palette-overrides
         '((blue "#THEME-BLUE")
           (string blue)
           (new-theme "#THEME-ONLY"))))
    (let ((base
           (alabaster-themes--palette-value
            'alabaster-themes-light))
          (combined
           (alabaster-themes--palette-value
            'alabaster-themes-light :overrides)))
      (list
       (mapcar
        (lambda (key)
          (list
           key
           (alabaster-themes--retrieve-palette-value key base)))
        '(blue green string new-common new-theme))
       (mapcar
        (lambda (key)
          (list
           key
           (alabaster-themes--retrieve-palette-value key combined)))
        '(blue green string fnname new-common new-theme))
       (seq-take combined 5)
       (list
        (length base)
        (length combined)
        (- (length combined) (length base))
        (= (length combined)
           (+ (length base) 6)))))))
"##,
        expect![[
            r##"OK (((blue "#325CC0") (green "#448C27") (string "#448C27") (new-common unspecified) (new-theme unspecified)) ((blue "#THEME-BLUE") (green "#COMMON-GREEN") (string "#THEME-BLUE") (fnname "#THEME-BLUE") (new-common "#COMMON-ONLY") (new-theme "#THEME-ONLY")) ((blue "#THEME-BLUE") (string blue) (new-theme "#THEME-ONLY") (blue "#COMMON-BLUE") (green "#COMMON-GREEN")) (108 114 6 t))"##
        ]],
    )
}

fn palette_preview_rows_preserve_mapping_status_values_and_color_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "palette_preview_rows_preserve_mapping_status_values_and_color_properties",
        r##"
(progn
  (load-theme 'alabaster-themes-light t t)
  (let* ((all
          (alabaster-themes--list-colors-tabulated
           'alabaster-themes-light))
         (mappings
          (alabaster-themes--list-colors-tabulated
           'alabaster-themes-light :mappings))
         (select
          (lambda (name entries)
            (let* ((entry (assq name entries))
                   (vector (cadr entry)))
              (list
               name
               (append vector nil)
               (mapcar
                (lambda (index)
                  (text-properties-at index (aref vector 2)))
                '(0)))))))
    (list
     (length all)
     (length mappings)
     (mapcar
      (lambda (name)
        (funcall select name all))
      '(bg-main blue string bg-fringe fg-term-red))
     (mapcar
      (lambda (name)
        (funcall select name mappings))
      '(err link string bg-fringe fg-term-red)))))
"##,
        expect![[
            r##"OK (108 42 ((bg-main ("" "bg-main" #("#F7F7F7" 0 7 (face (:foreground #1="#F7F7F7"))) #("#F7F7F7                       " 0 30 (face (:background #1# :foreground #5="black")))) ((face (:foreground "#F7F7F7")))) (blue ("" "blue" #("#325CC0" 0 7 (face (:foreground #2="#325CC0"))) #("#325CC0                       " 0 30 (face (:background #2# :foreground #4="#ffffff")))) ((face (:foreground "#325CC0")))) (string ("Yes" "string" #("green" 0 5 (face (:foreground #3="#448C27"))) #("green                         " 0 30 (face (:background #3# :foreground #4#)))) ((face (:foreground "#448C27")))) (bg-fringe ("" "bg-fringe" #("unspecified" 0 11 (face (:foreground unspecified))) #("unspecified                   " 0 30 (face (:background unspecified :foreground #5#)))) ((face (:foreground unspecified)))) (fg-term-red ("Yes" "fg-term-red" #("red" 0 3 (face (:foreground #6="#AA3731"))) #("red                           " 0 30 (face (:background #6# :foreground #4#)))) ((face (:foreground "#AA3731"))))) ((err ("Yes" "err" #("red" 0 3 (face (:foreground #6#))) #("red                           " 0 30 (face (:background #6# :foreground #4#)))) ((face (:foreground "#AA3731")))) (link ("Yes" "link" #("blue" 0 4 (face (:foreground #2#))) #("blue                          " 0 30 (face (:background #2# :foreground #4#)))) ((face (:foreground "#325CC0")))) (string ("Yes" "string" #("green" 0 5 (face (:foreground #3#))) #("green                         " 0 30 (face (:background #3# :foreground #4#)))) ((face (:foreground "#448C27")))) (bg-fringe ("" "bg-fringe" #("unspecified" 0 11 (face (:foreground unspecified))) #("unspecified                   " 0 30 (face (:background unspecified :foreground #5#)))) ((face (:foreground unspecified)))) (fg-term-red ("Yes" "fg-term-red" #("red" 0 3 (face (:foreground #6#))) #("red                           " 0 30 (face (:background #6# :foreground #4#)))) ((face (:foreground "#AA3731"))))))"##
        ]],
    )
}

fn heading_customization_combines_fallbacks_weights_pitch_heights_and_no_bold() -> ParityBatchCase {
    ParityBatchCase::value(
        "heading_customization_combines_fallbacks_weights_pitch_heights_and_no_bold",
        r##"
(let ((cases
       '((nil nil)
         (((1 light variable-pitch 1.5)
           (2 regular (height . 1.3))
           (3 . t)
           (t variable-pitch))
          nil)
         (((1 light variable-pitch 1.5)
           (2 regular (height . 1.3))
           (3 . t)
           (t variable-pitch))
          t))))
  (mapcar
   (lambda (case)
     (let ((alabaster-themes-headings (car case))
           (alabaster-themes-no-bold (cadr case)))
       (list
        alabaster-themes-no-bold
        (mapcar
         (lambda (level)
           (cons level
                 (alabaster-themes--heading level)))
         '(0 1 2 3 4 agenda-date))
        (alabaster-themes--weight
         '(variable-pitch semibold 1.2))
        (alabaster-themes--property-lookup
         '((height . 1.7) variable-pitch)
         'height #'floatp 'unspecified))))
   cases))
"##,
        expect![
            "OK ((nil ((0 :inherit bold :height unspecified :weight unspecified) (1 :inherit bold :height unspecified :weight unspecified) (2 :inherit bold :height unspecified :weight unspecified) (3 :inherit bold :height unspecified :weight unspecified) (4 :inherit bold :height unspecified :weight unspecified) (agenda-date :inherit bold :height unspecified :weight unspecified)) semibold 1.7) (nil ((0 :inherit (bold variable-pitch) :height unspecified :weight unspecified) (1 :inherit variable-pitch :height 1.5 :weight light) (2 :inherit nil :height 1.3 :weight regular) (3 :inherit bold :height unspecified :weight unspecified) (4 :inherit (bold variable-pitch) :height unspecified :weight unspecified) (agenda-date :inherit (bold variable-pitch) :height unspecified :weight unspecified)) semibold 1.7) (t ((0 :inherit variable-pitch :height unspecified :weight unspecified) (1 :inherit variable-pitch :height 1.5 :weight light) (2 :inherit nil :height 1.3 :weight regular) (3 :inherit default :height unspecified :weight unspecified) (4 :inherit variable-pitch :height unspecified :weight unspecified) (agenda-date :inherit variable-pitch :height unspecified :weight unspecified)) semibold 1.7))"
        ],
    )
}

fn font_option_helpers_cover_every_toggle_combination_without_hidden_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_option_helpers_cover_every_toggle_combination_without_hidden_state",
        r##"
(mapcar
 (lambda (case)
   (let ((alabaster-themes-mixed-fonts (nth 0 case))
         (alabaster-themes-variable-pitch-ui (nth 1 case))
         (alabaster-themes-no-bold (nth 2 case)))
     (list
      case
      (alabaster-themes--fixed-pitch)
      (alabaster-themes--variable-pitch-ui)
      (alabaster-themes--bold))))
 '((nil nil nil)
   (t nil nil)
   (nil t nil)
   (t t nil)
   (t t t)))
"##,
        expect![
            "OK (((nil nil nil) nil nil (:inherit bold)) ((t nil nil) (:inherit fixed-pitch) nil (:inherit bold)) ((nil t nil) nil (:inherit variable-pitch) (:inherit bold)) ((t t nil) (:inherit fixed-pitch) (:inherit variable-pitch) (:inherit bold)) ((t t t) (:inherit fixed-pitch) (:inherit variable-pitch) nil))"
        ],
    )
}

pub(super) fn palettes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        every_palette_has_complete_unique_ordered_and_fingerprinted_entries(),
        semantic_palette_mappings_recursively_resolve_for_all_five_variants(),
        terminal_foreground_and_background_slots_resolve_to_practical_ansi_tables(),
        palette_lookup_handles_literals_nested_aliases_unspecified_and_missing_keys(),
        per_theme_overrides_precede_common_overrides_without_mutating_base_palette(),
        palette_preview_rows_preserve_mapping_status_values_and_color_properties(),
        heading_customization_combines_fallbacks_weights_pitch_heights_and_no_bold(),
        font_option_helpers_cover_every_toggle_combination_without_hidden_state(),
    ]
}
