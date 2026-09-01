use expect_test::expect;

use super::ParityBatchCase;

fn alectryon_run_converter_sends_widened_input_and_exact_cli_arguments_to_a_real_executable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_run_converter_sends_widened_input_and_exact_cli_arguments_to_a_real_executable",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (script (expand-file-name "record-converter.sh" root))
       (arguments (expand-file-name "arguments.txt" root))
       (standard-input (expand-file-name "standard-input.txt" root))
       (input (generate-new-buffer " *alectryon input*")))
  (with-temp-file script
    (insert "#!/bin/sh\n"
            "printf '%s\\n' \"$@\" > \"$NEOMACS_TEST_SANDBOX_ROOT/arguments.txt\"\n"
            "cat > \"$NEOMACS_TEST_SANDBOX_ROOT/standard-input.txt\"\n"
            "printf 'converted document\\n'\n"))
  (set-file-modes script #o755)
  (unwind-protect
      (with-current-buffer input
        (insert "hidden prefix\nselected proof\nhidden suffix")
        (narrow-to-region 15 29)
        (let ((alectryon-executable script))
          (with-temp-buffer
            (alectryon--run-converter input '("--frontend" "coq+rst"
                                             "--backend" "rst"))
            (list (buffer-string)
                  (with-temp-buffer
                    (insert-file-contents arguments)
                    (buffer-string))
                  (with-temp-buffer
                    (insert-file-contents standard-input)
                    (buffer-string))))))
    (kill-buffer input)))"##,
        expect![[
            r#"OK ("converted document\n" "--frontend\ncoq+rst\n--backend\nrst\n--traceback\n-\n" "hidden prefix\nselected proof\nhidden suffix")"#
        ]],
    )
}

fn alectryon_run_converter_surfaces_real_nonzero_status_command_and_stderr_payload()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_run_converter_surfaces_real_nonzero_status_command_and_stderr_payload",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (script (expand-file-name "failing converter with spaces.sh" root))
       (input (generate-new-buffer " *invalid proof*")))
  (with-temp-file script
    (insert "#!/bin/sh\n"
            "cat >/dev/null\n"
            "printf 'line 7: malformed literate directive\\n'\n"
            "exit 23\n"))
  (set-file-modes script #o755)
  (unwind-protect
      (with-current-buffer input
        (insert "This input is malformed.")
        (let ((alectryon-executable script))
          (with-temp-buffer
            (condition-case err
                (progn
                  (alectryon--run-converter
                   input '("--frontend" "coq+rst" "--backend" "rst"))
                  'unexpected-success)
              (error
               (list (car err) (error-message-string err)
                     (buffer-string)))))))
    (kill-buffer input)))"##,
        expect![[
            r#"OK (error "Conversion error (23) when running ‘[ORACLE-SANDBOX]/failing\\ converter\\ with\\ spaces.sh --frontend coq\\+rst --backend rst --traceback -’:\nline 7: malformed literate directive\n" "line 7: malformed literate directive\n")"#
        ]],
    )
}

fn alectryon_run_converter_reports_a_practical_install_hint_for_missing_executables()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_run_converter_reports_a_practical_install_hint_for_missing_executables",
        r##"(let ((alectryon-executable
       (expand-file-name "not-installed/alectryon"
                         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
      (input (generate-new-buffer " *missing alectryon*")))
  (unwind-protect
      (with-temp-buffer
        (condition-case err
            (progn (alectryon--run-converter input nil) 'unexpected-success)
          (error
           (list (car err) (error-message-string err)))))
    (kill-buffer input)))"##,
        expect![[r#"OK (user-error "Alectryon binary not found; try ‘pip install alectryon’")"#]],
    )
}

fn alectryon_convert_from_uses_a_real_point_marker_restores_editability_and_widens_input()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_convert_from_uses_a_real_point_marker_restores_editability_and_widens_input",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (script (expand-file-name "point-converter.sh" root))
       (captured (expand-file-name "point-args.txt" root)))
  (with-temp-file script
    (insert "#!/bin/sh\n"
            "printf '%s\\n' \"$@\" > \"$NEOMACS_TEST_SANDBOX_ROOT/point-args.txt\"\n"
            "marker=''\n"
            "while [ \"$#\" -gt 0 ]; do\n"
            "  if [ \"$1\" = '--mark-point' ]; then marker=\"$3\"; shift 3; else shift; fi\n"
            "done\n"
            "cat >/dev/null\n"
            "printf 'Converted: AB%sCD\\n' \"$marker\"\n"))
  (set-file-modes script #o755)
  (with-temp-buffer
    (let ((alectryon--winding-down t))
      (coq-mode))
    (setq-local alectryon-prog-mode 'coq-mode
                alectryon-text-mode 'rst-mode)
    (insert "prefix\nCheck nat.\nsuffix")
    (goto-char 12)
    (narrow-to-region 8 18)
    (setq buffer-read-only t)
    (let ((alectryon-executable script))
      (alectryon--convert-from 'coq-mode))
    (list (buffer-string) (point) buffer-read-only
          (buffer-narrowed-p)
          (with-temp-buffer
            (insert-file-contents captured)
            (buffer-string)))))"##,
        expect![[
            r#"OK ("Converted: ABCD\n" 14 nil nil "--mark-point\n11\n￼127919￼\n--frontend\ncoq+rst\n--backend\nrst\n--traceback\n-\n")"#
        ]],
    )
}

fn alectryon_toggle_performs_a_real_code_to_markup_to_code_workflow_and_preserves_modified_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_toggle_performs_a_real_code_to_markup_to_code_workflow_and_preserves_modified_state",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (script (expand-file-name "roundtrip-converter.sh" root)))
  (with-temp-file script
    (insert "#!/bin/sh\n"
            "frontend=''; marker=''\n"
            "while [ \"$#\" -gt 0 ]; do\n"
            "  case \"$1\" in\n"
            "    --frontend) frontend=\"$2\"; shift 2 ;;\n"
            "    --mark-point) marker=\"$3\"; shift 3 ;;\n"
            "    *) shift ;;\n"
            "  esac\n"
            "done\n"
            "cat >/dev/null\n"
            "if [ \"$frontend\" = 'coq+rst' ]; then\n"
            "  printf 'A practical proof\\n=================\\n\\n.. coq::\\n\\n   Check %snat.\\n' \"$marker\"\n"
            "else\n"
            "  printf '(*|\\nA practical proof\\n=================\\n|*)\\n\\nCheck %snat.\\n' \"$marker\"\n"
            "fi\n"))
  (set-file-modes script #o755)
  (with-temp-buffer
    (let ((alectryon--winding-down t))
      (coq-mode))
    (setq-local alectryon-prog-mode 'coq-mode
                alectryon-text-mode 'rst-mode)
    (insert "(*|A practical proof|*)\nCheck nat.")
    (goto-char 34)
    (set-buffer-modified-p nil)
    (alectryon-mode 1)
    (let ((alectryon-executable script))
      (alectryon--toggle)
      (let ((markup
             (list major-mode (buffer-string) (point)
                   alectryon-mode alectryon--original-mode
                   (buffer-modified-p))))
        (alectryon--toggle)
        (list markup
              major-mode (buffer-string) (point)
              alectryon-mode alectryon--original-mode
              (buffer-modified-p)
              (consp buffer-undo-list))))))"##,
        expect![[
            r#"OK ((rst-mode "A practical proof\n=================\n\n.. coq::\n\n   Check nat.\n" 57 t coq-mode nil) coq-mode "(*|\nA practical proof\n=================\n|*)\n\nCheck nat.\n" 52 t coq-mode nil nil)"#
        ]],
    )
}

fn alectryon_save_writes_the_code_representation_while_preserving_the_markup_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_save_writes_the_code_representation_while_preserving_the_markup_buffer",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (script (expand-file-name "save-converter.sh" root))
       (document (expand-file-name "chapter_rst.v" root)))
  (with-temp-file script
    (insert "#!/bin/sh\n"
            "cat >/dev/null\n"
            "printf '(*|Saved from markup.|*)\\nCheck saved.\\n'\n"))
  (set-file-modes script #o755)
  (with-temp-file document
    (insert "old disk contents\n"))
  (with-temp-buffer
    (let ((alectryon--winding-down t))
      (rst-mode))
    (setq buffer-file-name document)
    (setq-local alectryon-prog-mode 'coq-mode
                alectryon-text-mode 'rst-mode
                alectryon--original-mode 'coq-mode)
    (insert "Saved from markup\n=================\n")
    (set-buffer-modified-p t)
    (let ((alectryon-executable script))
      (list
       (alectryon--save)
       major-mode
       (buffer-string)
       (buffer-modified-p)
       (with-temp-buffer
         (insert-file-contents document)
         (buffer-string))))))"##,
        expect![[
            r#"OK (t rst-mode "Saved from markup\n=================\n" nil "(*|Saved from markup.|*)\nCheck saved.\n")"#
        ]],
    )
}

fn alectryon_preview_builds_a_real_sandboxed_webpage_and_opens_the_generated_file()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_preview_builds_a_real_sandboxed_webpage_and_opens_the_generated_file",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (script (expand-file-name "preview-converter.sh" root))
       browsed)
  (with-temp-file script
    (insert "#!/bin/sh\n"
            "output=''\n"
            "while [ \"$#\" -gt 0 ]; do\n"
            "  if [ \"$1\" = '-o' ]; then output=\"$2\"; shift 2; else shift; fi\n"
            "done\n"
            "cat >/dev/null\n"
            "printf '<html><body><h1>Rendered proof</h1></body></html>\\n' > \"$output\"\n"
            "printf 'one proof rendered\\n'\n"))
  (set-file-modes script #o755)
  (with-temp-buffer
    (let ((alectryon--winding-down t))
      (coq-mode))
    (setq-local alectryon-prog-mode 'coq-mode
                alectryon-text-mode 'rst-mode)
    (insert "Check nat.")
    (let ((alectryon-executable script))
      (cl-letf (((symbol-function 'browse-url)
                 (lambda (url &rest _) (setq browsed url))))
        (alectryon-preview)))
    (list (file-name-directory browsed)
          (file-name-extension browsed)
          (file-readable-p browsed)
          (with-temp-buffer
            (insert-file-contents browsed)
            (buffer-string))
          (current-message))))"##,
        expect![[
            r#"OK ("[ORACLE-TMPDIR]/" "html" t "<html><body><h1>Rendered proof</h1></body></html>\n" nil)"#
        ]],
    )
}

pub(super) fn process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alectryon_run_converter_sends_widened_input_and_exact_cli_arguments_to_a_real_executable(),
        alectryon_run_converter_surfaces_real_nonzero_status_command_and_stderr_payload(),
        alectryon_run_converter_reports_a_practical_install_hint_for_missing_executables(),
        alectryon_convert_from_uses_a_real_point_marker_restores_editability_and_widens_input(),
        alectryon_toggle_performs_a_real_code_to_markup_to_code_workflow_and_preserves_modified_state(),
        alectryon_save_writes_the_code_representation_while_preserving_the_markup_buffer(),
        alectryon_preview_builds_a_real_sandboxed_webpage_and_opens_the_generated_file(),
    ]
}
