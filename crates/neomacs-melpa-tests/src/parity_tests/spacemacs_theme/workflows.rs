use expect_test::expect;

use super::ParityBatchCase;

/// Drive `load-theme' and assert the actual colors the theme DECLARES for the
/// default face (read from its theme-settings spec, since live face-attribute
/// is `unspecified' in batch). Validates the theme's real palette, not that it
/// merely registered.
fn default_face_declares_palette_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "default_face_declares_palette_colors",
        r####"
(progn
  (load-theme 'spacemacs-dark t)
  (list :bg (spc-theme-color 'default :background)
        :fg (spc-theme-color 'default :foreground)))
"####,
        expect![[r##"OK (:bg "#262626" :fg "#b2b2b2")"##]],
    )
}

/// Assert the foreground colors declared for core font-lock syntax faces.
fn font_lock_faces_declare_syntax_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_faces_declare_syntax_colors",
        r####"
(progn
  (load-theme 'spacemacs-dark t)
  (list :keyword (spc-theme-color 'font-lock-keyword-face :foreground)
        :string (spc-theme-color 'font-lock-string-face :foreground)
        :type (spc-theme-color 'font-lock-type-face :foreground)))
"####,
        expect![[r##"OK (:keyword "#268bd2" :string "#2aa198" :type "#df005f")"##]],
    )
}

/// Assert the background colors declared for region and cursor.
fn selection_faces_declare_background_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "selection_faces_declare_background_colors",
        r####"
(progn
  (load-theme 'spacemacs-dark t)
  (list :region (spc-theme-color 'region :background)
        :cursor (spc-theme-color 'cursor :background)))
"####,
        expect![[r##"OK (:region "#444444" :cursor "#d0d0d0")"##]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        default_face_declares_palette_colors(),
        font_lock_faces_declare_syntax_colors(),
        selection_faces_declare_background_colors(),
    ]
}
