use expect_test::expect;

use super::ParityBatchCase;

fn alectryon_exhaustively_maps_every_code_and_markup_pair_to_cli_frontends_and_backends()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_exhaustively_maps_every_code_and_markup_pair_to_cli_frontends_and_backends",
        r##"(let (records)
  (dolist (prog '(coq-mode lean4-mode dafny-mode))
    (dolist (text '(rst-mode markdown-mode typst-ts-mode))
      (with-temp-buffer
        (setq-local alectryon-prog-mode prog
                    alectryon-text-mode text)
        (push
         (list prog text
               (alectryon--config-code+markup)
               (alectryon--config-markup)
               (alectryon--config-frontend prog)
               (alectryon--config-backend prog)
               (alectryon--config-frontend text)
               (alectryon--config-backend text)
               (alectryon--converter-args prog)
               (alectryon--converter-args text))
         records))))
  (nreverse records))"##,
        expect![[
            r#"OK ((coq-mode rst-mode "coq+rst" "rst" "coq+rst" "rst" "rst" "coq+rst" ("--frontend" "coq+rst" "--backend" "rst") ("--frontend" "rst" "--backend" "coq+rst")) (coq-mode markdown-mode "coq+md" "md" "coq+md" "md" "md" "coq+md" ("--frontend" "coq+md" "--backend" "md") ("--frontend" "md" "--backend" "coq+md")) (coq-mode typst-ts-mode "coq+typst" "typst" "coq+typst" "typst" "typst" "coq+typst" ("--frontend" "coq+typst" "--backend" "typst") ("--frontend" "typst" "--backend" "coq+typst")) (lean4-mode rst-mode "lean4+rst" "rst" "lean4+rst" "rst" "rst" "lean4+rst" ("--frontend" "lean4+rst" "--backend" "rst") ("--frontend" "rst" "--backend" "lean4+rst")) (lean4-mode markdown-mode "lean4+md" "md" "lean4+md" "md" "md" "lean4+md" ("--frontend" "lean4+md" "--backend" "md") ("--frontend" "md" "--backend" "lean4+md")) (lean4-mode typst-ts-mode "lean4+typst" "typst" "lean4+typst" "typst" "typst" "lean4+typst" ("--frontend" "lean4+typst" "--backend" "typst") ("--frontend" "typst" "--backend" "lean4+typst")) (dafny-mode rst-mode "dafny+rst" "rst" "dafny+rst" "rst" "rst" "dafny+rst" ("--frontend" "dafny+rst" "--backend" "rst") ("--frontend" "rst" "--backend" "dafny+rst")) (dafny-mode markdown-mode "dafny+md" "md" "dafny+md" "md" "md" "dafny+md" ("--frontend" "dafny+md" "--backend" "md") ("--frontend" "md" "--backend" "dafny+md")) (dafny-mode typst-ts-mode "dafny+typst" "typst" "dafny+typst" "typst" "typst" "dafny+typst" ("--frontend" "dafny+typst" "--backend" "typst") ("--frontend" "typst" "--backend" "dafny+typst")))"#
        ]],
    )
}

fn alectryon_configuration_exposes_exact_delimiters_annotations_lint_and_exit_hooks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_configuration_exposes_exact_delimiters_annotations_lint_and_exit_hooks",
        r##"(let (records)
  (dolist (prog '(coq-mode lean4-mode dafny-mode))
    (with-temp-buffer
      (setq-local alectryon-prog-mode prog)
      (push
       (list prog
             (alectryon--config :tag 'prog)
             (alectryon--config :comment-delimiters 'prog)
             (alectryon--config :comment-delimiters-re 'prog)
             (alectryon--config :annotations-re 'prog)
             (alectryon--config :exit-hooks 'prog))
       records)))
  (dolist (text '(rst-mode markdown-mode typst-ts-mode))
    (with-temp-buffer
      (setq-local alectryon-text-mode text)
      (push
       (list text
             (alectryon--config :tag 'text)
             (alectryon--config :lint 'text)
             (alectryon--config :suffixes 'text))
       records)))
  (nreverse records))"##,
        expect![[
            r#"OK ((coq-mode "coq" ("(*|" . "|*)") ("([*]|" . "|[*])") "([*]\\(\\(?:\\s-*[.][-a-z]+\\)+\\)\\s-*[*])" (alectryon--coq-exit-hook)) (lean4-mode "lean4" ("/-|" . "|-/") ("/-|" . "|-/") "/-\\(\\(?:\\s-*[.][-a-z]+\\)+\\)\\s-*-/" nil) (dafny-mode "dafny" ("/// ") ("^///") nil nil) (rst-mode "rst" t ("_rst[.][^./]+$")) (markdown-mode "md" t ("_md[.][^./]+$")) (typst-ts-mode "typst" nil ("_typst[.][^./]+$")))"#
        ]],
    )
}

fn alectryon_guesses_markup_modes_from_real_project_filenames_and_buffer_names() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alectryon_guesses_markup_modes_from_real_project_filenames_and_buffer_names",
        r##"(mapcar
 (lambda (case)
   (with-temp-buffer
     (rename-buffer (car case) t)
     (setq buffer-file-name (cadr case))
     (list case (alectryon--guess-text-mode))))
 '(("chapter.v" "/project/decision_rst.v")
   ("chapter.v" "/project/decision_md.v")
   ("chapter.v" "/project/decision_typst.v")
   ("notes_rst.lean" nil)
   ("notes_md.dfy" nil)
   ("notes_typst.coq" nil)
   ("chapter.rst" "/project/chapter.rst")
   ("almost_rst" "/project/almost_rst")
   ("uppercase_RST.v" "/project/uppercase_RST.v")))"##,
        expect![[
            r#"OK ((("chapter.v" "/project/decision_rst.v") rst-mode) (("chapter.v" "/project/decision_md.v") markdown-mode) (("chapter.v" "/project/decision_typst.v") typst-ts-mode) (("notes_rst.lean" nil) rst-mode) (("notes_md.dfy" nil) markdown-mode) (("notes_typst.coq" nil) typst-ts-mode) (("chapter.rst" "/project/chapter.rst") nil) (("almost_rst" "/project/almost_rst") nil) (("uppercase_RST.v" "/project/uppercase_RST.v") rst-mode))"#
        ]],
    )
}

fn alectryon_selects_available_markup_modes_without_prompting_when_unambiguous() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alectryon_selects_available_markup_modes_without_prompting_when_unambiguous",
        r##"(let ((original alectryon-text-modes)
      records)
  (unwind-protect
      (dolist (modes
               '(((missing-mode :tag "x"))
                 ((missing-mode :tag "x") (rst-mode :tag "rst"))
                 ((rst-mode :tag "rst") (markdown-mode :tag "md")
                  (typst-ts-mode :tag "typst"))))
        (setq alectryon-text-modes modes)
        (push
         (list (alectryon--available-text-modes)
               (condition-case err
                   (cl-letf (((symbol-function 'completing-read)
                              (lambda (_prompt choices &rest _)
                                (car (last choices)))))
                     (alectryon--read-text-mode))
                 (error (list (car err) (error-message-string err)))))
         records))
    (setq alectryon-text-modes original))
  (nreverse records))"##,
        expect![[
            r#"OK ((nil (error "No supported text mode found")) (("rst-mode") rst-mode) (("rst-mode" "markdown-mode" "typst-ts-mode") typst-ts-mode))"#
        ]],
    )
}

fn alectryon_ensure_text_mode_prefers_explicit_then_filename_then_user_selection() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alectryon_ensure_text_mode_prefers_explicit_then_filename_then_user_selection",
        r##"(list
 (with-temp-buffer
   (setq-local alectryon-text-mode 'markdown-mode)
   (rename-buffer "proof_typst.v" t)
   (alectryon--ensure-text-mode-set)
   alectryon-text-mode)
 (with-temp-buffer
   (rename-buffer "proof_typst.v" t)
   (alectryon--ensure-text-mode-set)
   alectryon-text-mode)
 (with-temp-buffer
   (rename-buffer "proof.v" t)
   (cl-letf (((symbol-function 'alectryon--read-text-mode)
              (lambda () 'rst-mode)))
     (alectryon--ensure-text-mode-set))
   alectryon-text-mode)
 (with-temp-buffer
   (setq-local alectryon-text-mode nil
               alectryon-fallback-text-mode 'markdown-mode)
   (list (alectryon--text-mode-with-fallback)
         alectryon-text-mode)))"##,
        expect!["OK (markdown-mode typst-ts-mode rst-mode (markdown-mode nil))"],
    )
}

fn alectryon_mode_setters_update_configuration_and_switch_the_current_view_when_needed()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_mode_setters_update_configuration_and_switch_the_current_view_when_needed",
        r##"(list
 (with-temp-buffer
   (let ((alectryon--winding-down t))
     (coq-mode))
   (alectryon-set-prog-mode 'lean4-mode)
   (list major-mode alectryon-prog-mode alectryon-mode))
 (with-temp-buffer
   (let ((alectryon--winding-down t))
     (rst-mode))
   (setq-local alectryon-prog-mode 'coq-mode
               alectryon-text-mode 'rst-mode)
   (alectryon-set-text-mode 'markdown-mode)
   (list major-mode alectryon-text-mode alectryon-mode))
 (with-temp-buffer
   (let ((alectryon--winding-down t))
     (coq-mode))
   (setq-local alectryon-text-mode 'rst-mode)
   (alectryon-set-text-mode 'markdown-mode)
   (list major-mode alectryon-text-mode)))"##,
        expect![
            "OK ((lean4-mode lean4-mode t) (markdown-mode markdown-mode t) (coq-mode markdown-mode))"
        ],
    )
}

pub(super) fn configuration_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alectryon_exhaustively_maps_every_code_and_markup_pair_to_cli_frontends_and_backends(),
        alectryon_configuration_exposes_exact_delimiters_annotations_lint_and_exit_hooks(),
        alectryon_guesses_markup_modes_from_real_project_filenames_and_buffer_names(),
        alectryon_selects_available_markup_modes_without_prompting_when_unambiguous(),
        alectryon_ensure_text_mode_prefers_explicit_then_filename_then_user_selection(),
        alectryon_mode_setters_update_configuration_and_switch_the_current_view_when_needed(),
    ]
}
