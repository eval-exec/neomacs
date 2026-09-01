use expect_test::expect;

use super::ParityBatchCase;

/// Drive `load-theme' and assert the colors the theme DECLARES for the default
/// face (read from its theme-settings spec; live face-attribute is
/// `unspecified' in batch). Validates the solarized dark palette.
fn default_face_declares_palette_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "default_face_declares_palette_colors",
        r####"
(progn
  (load-theme 'sanityinc-solarized-dark t)
  (list :bg (ssol-theme-color 'default :background)
        :fg (ssol-theme-color 'default :foreground)))
"####,
        expect![[r##"OK (:bg "#002b36" :fg "#839496")"##]],
    )
}

/// Assert the foreground colors declared for core font-lock syntax faces.
fn font_lock_faces_declare_syntax_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_faces_declare_syntax_colors",
        r####"
(progn
  (load-theme 'sanityinc-solarized-dark t)
  (list :keyword (ssol-theme-color 'font-lock-keyword-face :foreground)
        :string (ssol-theme-color 'font-lock-string-face :foreground)
        :type (ssol-theme-color 'font-lock-type-face :foreground)))
"####,
        expect![[r##"OK (:keyword "#859900" :string "#2aa198" :type "#268bd2")"##]],
    )
}

/// Assert the background colors declared for region and cursor.
fn selection_faces_declare_background_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "selection_faces_declare_background_colors",
        r####"
(progn
  (load-theme 'sanityinc-solarized-dark t)
  (list :region (ssol-theme-color 'region :background)
        :cursor (ssol-theme-color 'cursor :background)))
"####,
        expect![[r##"OK (:region "#fdf6e3" :cursor "#d33682")"##]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        default_face_declares_palette_colors(),
        font_lock_faces_declare_syntax_colors(),
        selection_faces_declare_background_colors(),
    ]
}
