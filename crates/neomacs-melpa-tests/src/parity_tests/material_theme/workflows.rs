use expect_test::expect;

use super::ParityBatchCase;

/// Drive `load-theme' and assert the colors the material theme DECLARES for the
/// default face (read from theme-settings; live face-attribute is
/// `unspecified' in batch).
fn default_face_declares_palette_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "default_face_declares_palette_colors",
        r####"
(progn
  (load-theme 'material t)
  (list :bg (mat-theme-color 'default :background)
        :fg (mat-theme-color 'default :foreground)))
"####,
        expect![[r##"OK (:bg "#262626" :fg "#ffffff")"##]],
    )
}

/// Assert the foreground colors declared for core font-lock syntax faces.
fn font_lock_faces_declare_syntax_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_faces_declare_syntax_colors",
        r####"
(progn
  (load-theme 'material t)
  (list :keyword (mat-theme-color 'font-lock-keyword-face :foreground)
        :string (mat-theme-color 'font-lock-string-face :foreground)
        :type (mat-theme-color 'font-lock-type-face :foreground)))
"####,
        expect![[r##"OK (:keyword "#fff59d" :string "#9ccc65" :type "#84ffff")"##]],
    )
}

/// Assert the background colors declared for region and cursor.
fn selection_faces_declare_background_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "selection_faces_declare_background_colors",
        r####"
(progn
  (load-theme 'material t)
  (list :region (mat-theme-color 'region :background)
        :cursor (mat-theme-color 'cursor :background)))
"####,
        expect![[r##"OK (:region "#555555" :cursor "#ff9800")"##]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        default_face_declares_palette_colors(),
        font_lock_faces_declare_syntax_colors(),
        selection_faces_declare_background_colors(),
    ]
}
