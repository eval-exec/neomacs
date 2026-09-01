use expect_test::expect;

use super::ParityBatchCase;

fn palette_defaults_match_upstream_constants() -> ParityBatchCase {
    ParityBatchCase::value(
        "palette_defaults_match_upstream_constants",
        r####"
(list :fg monokai-foreground
      :bg monokai-background
      :yellow monokai-yellow
      :red monokai-red
      :green monokai-green
      :blue monokai-blue)
"####,
        expect![[
            r##"OK (:fg "#F8F8F2" :bg "#272822" :yellow "#E6DB74" :red "#F92672" :green "#A6E22E" :blue "#66D9EF")"##
        ]],
    )
}

fn load_theme_registers_and_enables_monokai() -> ParityBatchCase {
    ParityBatchCase::value(
        "load_theme_registers_and_enables_monokai",
        r####"
(progn
  (load-theme 'monokai t)
  (list :theme-p (and (custom-theme-p 'monokai) t)
        :enabled (and (custom-theme-enabled-p 'monokai) t)
        :in-enabled (and (memq 'monokai custom-enabled-themes) t)
        :feature (get 'monokai 'theme-feature)
        :doc (and (stringp (get 'monokai 'theme-documentation)) t)))
"####,
        expect![[r#"OK (:theme-p t :enabled t :in-enabled t :feature monokai-theme :doc t)"#]],
    )
}

fn theme_settings_include_default_and_font_lock_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "theme_settings_include_default_and_font_lock_faces",
        r####"
(progn
  (load-theme 'monokai t)
  (let ((faces
         (mapcar #'cadr
                 (cl-remove-if-not
                  (lambda (s) (eq (car s) 'theme-face))
                  (get 'monokai 'theme-settings)))))
    (list :has-default (and (memq 'default faces) t)
          :has-comment (and (memq 'font-lock-comment-face faces) t)
          :has-keyword (and (memq 'font-lock-keyword-face faces) t)
          :many-faces (> (length faces) 50))))
"####,
        expect![[r#"OK (:has-default t :has-comment t :has-keyword t :many-faces t)"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        palette_defaults_match_upstream_constants(),
        load_theme_registers_and_enables_monokai(),
        theme_settings_include_default_and_font_lock_faces(),
    ]
}
