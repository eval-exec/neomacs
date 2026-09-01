use expect_test::expect;

use super::ParityBatchCase;

fn opening_and_converting_a_real_ansi_art_file_visits_the_exported_png() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_and_converting_a_real_ansi_art_file_visits_the_exported_png",
        r##"(let* ((root
         (file-name-as-directory
          (expand-file-name
           "ansilove-file-workflow"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (input
         (expand-file-name "gallery/release.ans" root))
        (ansilove-temporary-directory
         (file-name-as-directory
          (expand-file-name "rendered" root)))
        (ansilove-executable nil)
        source-buffer
        result)
  (unwind-protect
      (progn
        (neomacs-ansilove-test-cleanup root)
        (setq ansilove-executable
              (neomacs-ansilove-test-write-converter root))
        (neomacs-ansilove-test-write-file
         input
         "\e[1;35mNEOMACS RELEASE\e[0m\n\e[36m████████\e[0m\n")
        (setq source-buffer (find-file-noselect input))
        (with-current-buffer source-buffer
          (let ((source-state
                 (list
                  major-mode
                  mode-name
                  buffer-read-only
                  (file-relative-name buffer-file-name root)
                  (buffer-string))))
            (ansilove)
            (let ((output buffer-file-name))
              (setq result
                    (list
                     source-state
                     (list
                      major-mode
                      mode-name
                      buffer-read-only
                      (point)
                      (buffer-size)
                      (neomacs-ansilove-test-output-name output root)
                      (image-type-from-file-name output))
                     (file-exists-p output)
                     (neomacs-ansilove-test-read-bytes output)
                     (with-current-buffer "*Ansilove-Output*"
                       (buffer-string))
                     (with-current-buffer source-buffer
                       (list
                        major-mode
                        buffer-read-only
                        (buffer-modified-p)
                        (buffer-string)))))))))
    (neomacs-ansilove-test-cleanup root))
  result)"##,
        expect![[
            r#"OK ((ansilove-mode "ansilove" t "gallery/release.ans" "\33[1;35mNEOMACS RELEASE\33[0m\n\33[36m████████\33[0m\n") (fundamental-mode "Fundamental" nil 1 68 "rendered/ansilove_<id>.png" png) t (137 80 78 71 13 10 26 10 0 0 0 13 73 72 68 82 0 0 0 1 0 0 0 1 8 4 0 0 0 181 28 12 2 0 0 0 11 73 68 65 84 120 218 99 100 248 15 0 1 5 1 1 39 24 227 102 0 0 0 0 73 69 78 68 174 66 96 130) "ansilove-fixture: converted 61 bytes\n" (ansilove-mode t nil "\33[1;35mNEOMACS RELEASE\33[0m\n\33[36m████████\33[0m\n"))"#
        ]],
    )
}

fn converting_an_unsaved_art_buffer_exports_png_and_removes_the_staged_text_file() -> ParityBatchCase
{
    ParityBatchCase::value(
        "converting_an_unsaved_art_buffer_exports_png_and_removes_the_staged_text_file",
        r##"(let* ((root
         (file-name-as-directory
          (expand-file-name
           "ansilove-buffer-workflow"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (ansilove-temporary-directory
         (file-name-as-directory
          (expand-file-name "rendered" root)))
        (ansilove-executable nil)
        source-buffer
        result)
  (unwind-protect
      (progn
        (neomacs-ansilove-test-cleanup root)
        (setq ansilove-executable
              (neomacs-ansilove-test-write-converter root))
        (setq source-buffer
              (generate-new-buffer " *ansilove-unsaved-art*"))
        (with-current-buffer source-buffer
          (insert
           "╔══════════════╗\n"
           "║ \e[1;32mBUILD READY\e[0m ║\n"
           "╚══════════════╝\n")
          (goto-char 5)
          (ansilove-convert-and-display-now)
          (let ((output buffer-file-name))
            (setq result
                  (list
                   (with-current-buffer source-buffer
                     (list
                      (buffer-name)
                      buffer-file-name
                      major-mode
                      (point)
                      (buffer-modified-p)
                      (buffer-string)))
                   (list
                    major-mode
                    (point)
                    (buffer-size)
                    (neomacs-ansilove-test-output-name output root))
                   (mapcar
                    (lambda (file)
                      (neomacs-ansilove-test-output-name file root))
                    (directory-files-recursively
                     ansilove-temporary-directory
                     ".*"
                     nil))
                   (neomacs-ansilove-test-file-summary output)
                   (with-current-buffer "*Ansilove-Output*"
                     (buffer-string))
                   (let (staged-buffers)
                     (dolist (buffer (buffer-list))
                       (when
                           (string-match-p
                            "\\`ansilove_[0-9]+\\.txt\\'"
                            (buffer-name buffer))
                         (push (buffer-name buffer) staged-buffers)))
                     (nreverse staged-buffers)))))))
    (when (buffer-live-p source-buffer)
      (with-current-buffer source-buffer
        (set-buffer-modified-p nil))
      (kill-buffer source-buffer))
    (neomacs-ansilove-test-cleanup root))
  result)"##,
        expect![[
            r#"OK ((" *ansilove-unsaved-art*" nil fundamental-mode 5 t "╔══════════════╗\n║ \33[1;32mBUILD READY\33[0m ║\n╚══════════════╝\n") (fundamental-mode 1 68 "rendered/ansilove_<id>.png") ("rendered/ansilove_<id>.png") (68 (137 80 78 71 13 10 26 10)) "ansilove-fixture: converted 129 bytes\n" nil)"#
        ]],
    )
}

fn editing_saving_and_reconverting_art_cleans_the_previous_export_before_viewing_the_new_one()
-> ParityBatchCase {
    ParityBatchCase::value(
        "editing_saving_and_reconverting_art_cleans_the_previous_export_before_viewing_the_new_one",
        r##"(let* ((root
         (file-name-as-directory
          (expand-file-name
           "ansilove-edit-workflow"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (input
         (expand-file-name "gallery/dashboard.ans" root))
        (ansilove-temporary-directory
         (file-name-as-directory
          (expand-file-name "rendered" root)))
        (ansilove-executable nil)
        source-buffer
        first-output
        initial-state
        editable-state
        saved-state
        first-state
        result)
  (unwind-protect
      (progn
        (neomacs-ansilove-test-cleanup root)
        (setq ansilove-executable
              (neomacs-ansilove-test-write-converter root))
        (neomacs-ansilove-test-write-file
         input
         "\e[1;34mDEPLOYMENT DASHBOARD\e[0m\nstatus: waiting\n")
        (setq source-buffer (find-file-noselect input))
        (with-current-buffer source-buffer
          (setq initial-state
                (list major-mode buffer-read-only (buffer-string)))
          (ansilove-turn-to-editable-mode)
          (setq editable-state
                (list major-mode buffer-read-only (buffer-modified-p)))
          (goto-char (point-max))
          (insert "target: production\n")
          (save-buffer)
          (setq saved-state
                (list
                 major-mode
                 buffer-read-only
                 (buffer-modified-p)
                 (buffer-string)))
          (ansilove-mode)
          (let ((ansilove-clean-temporary-directory-before-conversion
                 nil))
            (ansilove)
            (setq first-output buffer-file-name))
          (setq first-state
                (list
                 (neomacs-ansilove-test-output-name
                  first-output
                  root)
                 (file-exists-p first-output)))
          (set-buffer source-buffer)
          (let ((ansilove-clean-temporary-directory-before-conversion
                 t))
            (ansilove)
            (let ((second-output buffer-file-name))
              (setq result
                    (list
                     initial-state
                     editable-state
                     saved-state
                     (with-current-buffer source-buffer
                       (list
                        major-mode
                        buffer-read-only
                        (buffer-modified-p)
                        (buffer-string)))
                     first-state
                     (list
                      (file-exists-p first-output)
                      (neomacs-ansilove-test-output-name
                       second-output
                       root)
                      (file-exists-p second-output)
                      (neomacs-ansilove-test-file-summary
                       second-output))
                     (mapcar
                      (lambda (file)
                        (neomacs-ansilove-test-output-name
                         file
                         root))
                      (directory-files-recursively
                       ansilove-temporary-directory
                       ".*"
                       nil))
                     (with-current-buffer
                         "*Ansilove-Output*"
                       (buffer-string))
                     (with-temp-buffer
                       (insert-file-contents-literally input)
                       (buffer-string))))))))
    (neomacs-ansilove-test-cleanup root))
  result)"##,
        expect![[
            r#"OK ((ansilove-mode t "\33[1;34mDEPLOYMENT DASHBOARD\33[0m\nstatus: waiting\n") (fundamental-mode nil nil) (fundamental-mode nil nil "\33[1;34mDEPLOYMENT DASHBOARD\33[0m\nstatus: waiting\ntarget: production\n") (ansilove-mode t nil "\33[1;34mDEPLOYMENT DASHBOARD\33[0m\nstatus: waiting\ntarget: production\n") ("rendered/ansilove_<id>.png" t) (nil "rendered/ansilove_<id>.png" t (68 (137 80 78 71 13 10 26 10))) ("rendered/ansilove_<id>.png") "ansilove-fixture: converted 67 bytes\nansilove-fixture: converted 67 bytes\n" "\33[1;34mDEPLOYMENT DASHBOARD\33[0m\nstatus: waiting\ntarget: production\n")"#
        ]],
    )
}

fn quick_example_downloads_local_art_once_then_reuses_it_for_a_second_real_conversion()
-> ParityBatchCase {
    ParityBatchCase::value(
        "quick_example_downloads_local_art_once_then_reuses_it_for_a_second_real_conversion",
        r##"(let* ((root
         (file-name-as-directory
          (expand-file-name
           "ansilove-quick-workflow"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (example
         (expand-file-name "fixtures/example.ans" root))
        (ansilove-temporary-directory
         (file-name-as-directory
          (expand-file-name "session" root)))
        (ansilove-executable nil)
        (ansilove-quick-test-example-url
         (concat "file://" example))
        first-result
        second-result
        result)
  (unwind-protect
      (progn
        (neomacs-ansilove-test-cleanup root)
        (setq ansilove-executable
              (neomacs-ansilove-test-write-converter root))
        (neomacs-ansilove-test-write-file
         example
         "\e[1;33mLOCAL EXAMPLE\e[0m\nframe: one\n")
        (setq first-result (ansilove-quick-test-example))
        (neomacs-ansilove-test-write-file
         example
         "\e[1;31mREMOTE CHANGED\e[0m\nframe: two\n")
        (setq second-result (ansilove-quick-test-example))
        (let* ((cached
                (expand-file-name
                 "test.txt"
                 ansilove-temporary-directory))
               (exports
                (directory-files-recursively
                 ansilove-temporary-directory
                 ".*\\.png\\'"
                 nil)))
          (setq result
                (list
                 (list
                  (bufferp first-result)
                  (with-current-buffer first-result
                    (list
                     major-mode
                     (point)
                     (buffer-size)
                     (neomacs-ansilove-test-output-name
                      buffer-file-name
                      root))))
                 (list
                  (bufferp second-result)
                  (with-current-buffer second-result
                    (list
                     major-mode
                     (point)
                     (buffer-size)
                     (neomacs-ansilove-test-output-name
                      buffer-file-name
                      root))))
                 (with-current-buffer
                     (get-file-buffer cached)
                   (list
                    major-mode
                    buffer-read-only
                    (buffer-modified-p)
                    (buffer-string)))
                 (with-temp-buffer
                   (insert-file-contents-literally cached)
                   (buffer-string))
                 (with-temp-buffer
                   (insert-file-contents-literally example)
                   (buffer-string))
                 (length exports)
                 (mapcar
                  (lambda (file)
                    (neomacs-ansilove-test-output-name
                     file
                     root))
                  exports)
                 (mapcar
                  #'neomacs-ansilove-test-file-summary
                  exports)
                 (with-current-buffer "*Ansilove-Output*"
                   (buffer-string))))))
    (neomacs-ansilove-test-cleanup root))
  result)"##,
        expect![[
            r#"OK ((t (fundamental-mode 1 68 "session/ansilove_<id>.png")) (t (fundamental-mode 1 68 "session/ansilove_<id>.png")) (ansilove-mode t nil "\33[1;33mLOCAL EXAMPLE\33[0m\nframe: one\n") "\33[1;33mLOCAL EXAMPLE\33[0m\nframe: one\n" "\33[1;31mREMOTE CHANGED\33[0m\nframe: two\n" 2 ("session/ansilove_<id>.png" "session/ansilove_<id>.png") ((68 (137 80 78 71 13 10 26 10)) (68 (137 80 78 71 13 10 26 10))) "ansilove-fixture: converted 36 bytes\nansilove-fixture: converted 36 bytes\n")"#
        ]],
    )
}

pub(super) fn practical_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_and_converting_a_real_ansi_art_file_visits_the_exported_png(),
        converting_an_unsaved_art_buffer_exports_png_and_removes_the_staged_text_file(),
        editing_saving_and_reconverting_art_cleans_the_previous_export_before_viewing_the_new_one(),
        quick_example_downloads_local_art_once_then_reuses_it_for_a_second_real_conversion(),
    ]
}
