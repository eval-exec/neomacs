use expect_test::expect;

use super::ParityBatchCase;

fn readme_family_customization_changes_the_font_used_by_real_icons() -> ParityBatchCase {
    ParityBatchCase::value(
        "readme_family_customization_changes_the_font_used_by_real_icons",
        r##"(let* ((all-the-icons-nerd-fonts-family
                      "Project Nerd Symbols")
                     (icon
                      (all-the-icons-nerd-fa
                       "github"
                       :face 'success
                       :height 1.5
                       :v-adjust 0.1)))
               (list
                (substring-no-properties icon)
                (string-to-list
                 (substring-no-properties icon))
                (all-the-icons-icon-family icon)
                (get-text-property 0 'face icon)
                (get-text-property 0 'font-lock-face icon)
                (get-text-property 0 'display icon)))"##,
        expect![[
            r#"OK ("" (61595) "Project Nerd Symbols" #1=(:family "Project Nerd Symbols" :height 1.7999999999999998 :inherit success) #1# (raise 0.12))"#
        ]],
    )
}

fn application_palette_renders_real_language_file_terminal_weather_and_powerline_icons()
-> ParityBatchCase {
    ParityBatchCase::value(
        "application_palette_renders_real_language_file_terminal_weather_and_powerline_icons",
        r##"(mapcar
               (lambda (spec)
                 (let* ((family (nth 0 spec))
                        (name (nth 1 spec))
                        (face (nth 2 spec))
                        (icon
                         (funcall
                          family
                          name
                          :face face)))
                   (list
                    family
                    name
                    (substring-no-properties icon)
                    (string-to-list
                     (substring-no-properties icon))
                    (all-the-icons-icon-family icon)
                    (get-text-property 0 'face icon)
                    (get-text-property 0 'display icon))))
               '((all-the-icons-nerd-fa
                  "github" font-lock-constant-face)
                 (all-the-icons-nerd-md
                  "language-rust" font-lock-type-face)
                 (all-the-icons-nerd-cod
                  "terminal" font-lock-builtin-face)
                 (all-the-icons-nerd-dev
                  "python" font-lock-keyword-face)
                 (all-the-icons-nerd-oct
                  "file" font-lock-string-face)
                 (all-the-icons-nerd-weather
                  "day-sunny" warning)
                 (all-the-icons-nerd-seti
                  "javascript" font-lock-variable-name-face)
                 (all-the-icons-nerd-custom
                  "c" font-lock-function-name-face)
                 (all-the-icons-nerd-linux
                  "docker" font-lock-doc-face)
                 (all-the-icons-nerd-pl
                  "right-hard-divider" shadow)))"##,
        expect![[
            r#"OK ((all-the-icons-nerd-fa "github" "" (61595) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-constant-face) (raise -0.24)) (all-the-icons-nerd-md "language-rust" "󱘗" (988695) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-type-face) (raise -0.24)) (all-the-icons-nerd-cod "terminal" "" (60037) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-builtin-face) (raise -0.24)) (all-the-icons-nerd-dev "python" "" (59196) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-keyword-face) (raise -0.24)) (all-the-icons-nerd-oct "file" "" (62629) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-string-face) (raise -0.24)) (all-the-icons-nerd-weather "day-sunny" "" (58125) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit warning) (raise -0.24)) (all-the-icons-nerd-seti "javascript" "" (58892) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-variable-name-face) (raise -0.24)) (all-the-icons-nerd-custom "c" "" (58910) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-function-name-face) (raise -0.24)) (all-the-icons-nerd-linux "docker" "" (62216) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-doc-face) (raise -0.24)) (all-the-icons-nerd-pl "right-hard-divider" "" (57522) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit shadow) (raise -0.24)))"#
        ]],
    )
    .fresh_process()
}

fn normalized_hyphenated_names_render_icons_whose_nerd_data_uses_underscores() -> ParityBatchCase {
    ParityBatchCase::value(
        "normalized_hyphenated_names_render_icons_whose_nerd_data_uses_underscores",
        r##"(mapcar
               (lambda (spec)
                 (let ((icon
                        (funcall
                         (car spec)
                         (cadr spec))))
                   (list
                    spec
                    (substring-no-properties icon)
                    (string-to-list
                     (substring-no-properties icon))
                    (all-the-icons-icon-family icon))))
               '((all-the-icons-nerd-md
                  "format-align-left")
                 (all-the-icons-nerd-custom
                  "common-lisp")
                 (all-the-icons-nerd-pl
                  "left-hard-divider")
                 (all-the-icons-nerd-ple
                  "pixelated-squares-small-mirrored")))"##,
        expect![[
            r#"OK (((all-the-icons-nerd-md "format-align-left") "󰉢" (983650) "Symbols Nerd Font") ((all-the-icons-nerd-custom "common-lisp") "" (59056) "Symbols Nerd Font") ((all-the-icons-nerd-pl "left-hard-divider") "" (57520) "Symbols Nerd Font") ((all-the-icons-nerd-ple "pixelated-squares-small-mirrored") "" (57541) "Symbols Nerd Font"))"#
        ]],
    )
}

fn family_renderer_preserves_real_face_height_adjust_and_flip_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "family_renderer_preserves_real_face_height_adjust_and_flip_arguments",
        r##"(let ((icon
                      (all-the-icons-nerd-fa
                       "github"
                       :face 'font-lock-keyword-face
                       :height 1.75
                       :v-adjust -0.2
                       :flip 'horizontal)))
               (list
                (substring-no-properties icon)
                (string-to-list
                 (substring-no-properties icon))
                (all-the-icons-icon-family icon)
                (text-properties-at 0 icon)
                (get-text-property 0 'display icon)))"##,
        expect![[
            r#"OK ("" (61595) "Symbols Nerd Font" (face #1=(:family "Symbols Nerd Font" :height 2.1 :inherit font-lock-keyword-face) font-lock-face #1# display #2=(raise -0.24) rear-nonsticky t) #2#)"#
        ]],
    )
    .fresh_process()
}

fn unknown_icon_name_reports_the_real_family_and_candidate_to_the_user() -> ParityBatchCase {
    ParityBatchCase::value(
        "unknown_icon_name_reports_the_real_family_and_candidate_to_the_user",
        r##"(condition-case error
               (all-the-icons-nerd-fa
                "definitely-missing")
             (error
              (list
               (car error)
               (error-message-string error))))"##,
        expect![[
            r#"OK (error "Unable to find icon with name ‘definitely-missing’ in icon set ‘nerd-fa’")"#
        ]],
    )
}

pub(super) fn families_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        readme_family_customization_changes_the_font_used_by_real_icons(),
        application_palette_renders_real_language_file_terminal_weather_and_powerline_icons(),
        normalized_hyphenated_names_render_icons_whose_nerd_data_uses_underscores(),
        family_renderer_preserves_real_face_height_adjust_and_flip_arguments(),
        unknown_icon_name_reports_the_real_family_and_candidate_to_the_user(),
    ]
}
