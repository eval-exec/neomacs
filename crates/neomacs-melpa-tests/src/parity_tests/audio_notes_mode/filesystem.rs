use expect_test::expect;

use super::ParityBatchCase;

fn audio_notes_mode_real_directory_scan_filters_supported_files_and_keeps_matching_directories()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_real_directory_scan_filters_supported_files_and_keeps_matching_directories",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "scan"))
                                (anm/notes-directory directory))
                           (dolist
                               (name
                                '("bravo.wav"
                                  "alpha.mp3"
                                  "charlie.mp4"
                                  "delta.3ga"
                                  "echo.3gpp"
                                  "foxtrot.m4a"
                                  ".hidden.mp3"
                                  "upper.MP3"
                                  "plain"
                                  "image.png"))
                             (audio-notes-test-write
                              directory
                              name
                              name))
                           (make-directory
                            (expand-file-name
                             "folder.wav"
                             directory))
                           (mapcar
                            (lambda (path)
                              (list
                               (file-name-nondirectory path)
                               (file-regular-p path)
                               (file-directory-p path)))
                            (anm/list-files)))"##,
        expect![[
            r#"OK (("alpha.mp3" t nil) ("bravo.wav" t nil) ("charlie.mp4" t nil) ("delta.3ga" t nil) ("echo.3gpp" t nil) ("folder.wav" nil t) ("foxtrot.m4a" t nil))"#
        ]],
    )
}

fn audio_notes_mode_live_custom_regexp_controls_real_directory_selection() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_live_custom_regexp_controls_real_directory_selection",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "custom-regexp"))
                                (anm/notes-directory directory))
                           (dolist
                               (name
                                '("memo.opus"
                                  "memo.wav"
                                  "voice-01.txt"
                                  "voice-02.txt"
                                  ".voice-03.txt"))
                             (audio-notes-test-write
                              directory
                              name
                              name))
                           (let ((defaults
                                  (mapcar
                                   #'file-name-nondirectory
                                   (anm/list-files)))
                                 (anm/file-regexp
                                  "\\`voice-[0-9]+\\.txt\\'"))
                             (list
                              defaults
                              (mapcar
                               #'file-name-nondirectory
                               (anm/list-files))
                              anm/file-regexp)))"##,
        expect![[
            r#"OK (("memo.wav") ("voice-01.txt" "voice-02.txt") "\\`voice-[0-9]+\\.txt\\'")"#
        ]],
    )
}

fn audio_notes_mode_global_mode_string_counts_real_notes_and_preserves_face_property()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_global_mode_string_counts_real_notes_and_preserves_face_property",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "modeline-count"))
                                (anm/notes-directory directory)
                                (anm/mode-line-color "#12ab34"))
                           (let ((empty
                                  (anm/global-mode-string)))
                             (audio-notes-test-write
                              directory
                              "01.wav"
                              "first")
                             (audio-notes-test-write
                              directory
                              "02.m4a"
                              "second")
                             (audio-notes-test-write
                              directory
                              ".ignored.mp3"
                              "hidden")
                             (let ((two
                                    (anm/global-mode-string)))
                               (delete-file
                                (expand-file-name
                                 "01.wav"
                                 directory))
                               (let ((one
                                      (anm/global-mode-string)))
                                 (list
                                  empty
                                  (audio-notes-test-face-property
                                   two)
                                  (audio-notes-test-face-property
                                   one))))))"##,
        expect![[
            r##"OK (nil ("2 Notes" (:foreground "#12ab34")) ("1 Notes" (:foreground "#12ab34")))"##
        ]],
    )
}

fn audio_notes_mode_noninteractive_modeline_control_is_idempotent_and_updates_color_only_for_strings()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_noninteractive_modeline_control_is_idempotent_and_updates_color_only_for_strings",
        r##"(let ((global-mode-string
                                '("left" (:eval unrelated) "right"))
                               (anm/mode-line-color
                                "ForestGreen"))
                           (let ((add-t
                                  (progn
                                    (anm/display-on-modeline t)
                                    (copy-tree
                                     global-mode-string)))
                                 (color-after-t
                                  anm/mode-line-color))
                             (let ((add-again
                                    (progn
                                      (anm/display-on-modeline
                                       :truthy)
                                      (copy-tree
                                       global-mode-string)))
                                   (color-after-truthy
                                    anm/mode-line-color))
                               (let ((add-color
                                      (progn
                                        (anm/display-on-modeline
                                         "DeepSkyBlue")
                                        (copy-tree
                                         global-mode-string)))
                                     (color-after-string
                                      anm/mode-line-color))
                                 (let ((remove
                                        (progn
                                          (anm/display-on-modeline
                                           nil)
                                          (copy-tree
                                           global-mode-string))))
                                   (list
                                    add-t
                                    color-after-t
                                    add-again
                                    color-after-truthy
                                    add-color
                                    color-after-string
                                    remove
                                    anm/mode-line-color))))))"##,
        expect![[
            r#"OK (((:eval (anm/global-mode-string)) "left" (:eval unrelated) "right") "ForestGreen" ((:eval (anm/global-mode-string)) "left" (:eval unrelated) "right") "ForestGreen" ((:eval (anm/global-mode-string)) "left" (:eval unrelated) "right") "DeepSkyBlue" ("left" (:eval unrelated) "right") "DeepSkyBlue")"#
        ]],
    )
}

fn audio_notes_mode_interactive_modeline_command_toggles_exact_entry_without_changing_color()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_interactive_modeline_command_toggles_exact_entry_without_changing_color",
        r##"(let ((global-mode-string
                                '("base"))
                               (anm/mode-line-color
                                "OrangeRed"))
                           (let ((first
                                  (progn
                                    (call-interactively
                                     #'anm/display-on-modeline)
                                    (copy-tree
                                     global-mode-string))))
                             (let ((second
                                    (progn
                                      (call-interactively
                                       #'anm/display-on-modeline)
                                      (copy-tree
                                       global-mode-string))))
                               (let ((third
                                      (progn
                                        (call-interactively
                                         #'anm/display-on-modeline)
                                        (copy-tree
                                         global-mode-string))))
                                 (list
                                  first
                                  second
                                  third
                                  anm/mode-line-color)))))"##,
        expect![[
            r#"OK (((:eval (anm/global-mode-string)) "base") ("base") ((:eval (anm/global-mode-string)) "base") "OrangeRed")"#
        ]],
    )
}

fn audio_notes_mode_play_next_deletes_real_current_file_then_requests_next_playback()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_play_next_deletes_real_current_file_then_requests_next_playback",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "delete-current"))
                                (current
                                 (audio-notes-test-write
                                  directory
                                  "current.wav"
                                  "audio"))
                                (anm/current current)
                                (anm/delete-command
                                 '(delete-file file t))
                                calls)
                           (cl-letf
                               (((symbol-function
                                  'anm/play-current)
                                 (lambda ()
                                   (push
                                    (list
                                     :play
                                     anm/current)
                                    calls)
                                   :played-next)))
                             (list
                              (anm/play-next)
                              anm/current
                              (file-exists-p current)
                              (nreverse calls))))"##,
        expect!["OK (:played-next nil nil ((:play nil)))"],
    )
}

fn audio_notes_mode_play_next_supports_practical_custom_archive_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_play_next_supports_practical_custom_archive_command",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "archive-current"))
                                (archive
                                 (audio-notes-test-directory
                                  "archive-destination"))
                                (current
                                 (audio-notes-test-write
                                  directory
                                  "memo.m4a"
                                  "voice memo"))
                                (destination
                                 (expand-file-name
                                  "memo.done"
                                  archive))
                                (anm/current current)
                                (anm/delete-command
                                 `(rename-file
                                   file
                                   ,destination))
                                calls)
                           (cl-letf
                               (((symbol-function
                                  'anm/play-current)
                                 (lambda ()
                                   (push :play-next calls)
                                   :next)))
                             (list
                              (anm/play-next)
                              anm/current
                              (file-exists-p current)
                              (file-exists-p destination)
                              (with-temp-buffer
                                (insert-file-contents
                                 destination)
                                (buffer-string))
                              (nreverse calls))))"##,
        expect![[r#"OK (:next nil nil t "voice memo" (:play-next))"#]],
    )
}

fn audio_notes_mode_play_next_warns_for_missing_current_but_continues_workflow() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audio_notes_mode_play_next_warns_for_missing_current_but_continues_workflow",
        r##"(let ((anm/current
                                (expand-file-name
                                 "missing.wav"
                                 default-directory))
                               calls)
                           (cl-letf
                               (((symbol-function
                                  'anm/play-current)
                                 (lambda ()
                                   (push :play-next calls)
                                   :continued)))
                             (list
                              (audio-notes-test-warning
                               (lambda ()
                                 (anm/play-next)))
                              anm/current
                              (nreverse calls))))"##,
        expect![[
            r#"OK ((:continued ((emacs "File [ORACLE-SANDBOX]/missing.wav not found for deletion." nil nil))) "[ORACLE-SANDBOX]/missing.wav" (:play-next))"#
        ]],
    )
}

fn audio_notes_mode_play_next_unwritable_file_disables_mode_and_signals_exact_error()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_play_next_unwritable_file_disables_mode_and_signals_exact_error",
        r##"(let ((anm/current
                                "/fixed/read-only.wav")
                               calls)
                           (cl-letf
                               (((symbol-function
                                  'file-readable-p)
                                 (lambda (file)
                                   (push
                                    (list :readable file)
                                    calls)
                                   t))
                                ((symbol-function
                                  'file-writable-p)
                                 (lambda (file)
                                   (push
                                    (list :writable file)
                                    calls)
                                   nil))
                                ((symbol-function
                                  'audio-notes-mode)
                                 (lambda (&optional argument)
                                   (push
                                    (list :mode argument)
                                    calls)
                                   :disabled))
                                ((symbol-function
                                  'anm/play-current)
                                 (lambda ()
                                   (push :unexpected-play calls)
                                   :played)))
                             (list
                              (audio-notes-test-error
                               (lambda ()
                                 (anm/play-next)))
                              anm/current
                              (nreverse calls))))"##,
        expect![[
            r#"OK ((:signal error ("File /fixed/read-only.wav can’t be deleted.\nCheck file permissions and fix this.\n(Exiting)")) "/fixed/read-only.wav" ((:readable "/fixed/read-only.wav") (:writable "/fixed/read-only.wav") (:mode -1)))"#
        ]],
    )
}

pub(super) fn filesystem_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audio_notes_mode_real_directory_scan_filters_supported_files_and_keeps_matching_directories(),
        audio_notes_mode_live_custom_regexp_controls_real_directory_selection(),
        audio_notes_mode_global_mode_string_counts_real_notes_and_preserves_face_property(),
        audio_notes_mode_noninteractive_modeline_control_is_idempotent_and_updates_color_only_for_strings(),
        audio_notes_mode_interactive_modeline_command_toggles_exact_entry_without_changing_color(),
        audio_notes_mode_play_next_deletes_real_current_file_then_requests_next_playback(),
        audio_notes_mode_play_next_supports_practical_custom_archive_command(),
        audio_notes_mode_play_next_warns_for_missing_current_but_continues_workflow(),
        audio_notes_mode_play_next_unwritable_file_disables_mode_and_signals_exact_error(),
    ]
}
