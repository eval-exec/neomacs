use expect_test::expect;

use super::ParityBatchCase;

fn audio_notes_mode_external_player_expands_file_arguments_replaces_live_process_and_disables_exit_query()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_external_player_expands_file_arguments_replaces_live_process_and_disables_exit_query",
        r##"(progn
                          (require 'cl)
                          (let* ((directory
                                 (audio-notes-test-directory
                                  "external-player"))
                                (file
                                 (audio-notes-test-write
                                  directory
                                  "voice memo.wav"
                                  "audio"))
                                (anm/player-command
                                 '("mock-player"
                                   "--quiet"
                                   file
                                   "--again"
                                   file))
                                (anm/process-buffer
                                 'fake-process-buffer)
                                (anm/process
                                 'old-process)
                                events)
                           (cl-letf
                               (((symbol-function
                                  'process-status)
                                 (lambda (process)
                                   (push
                                    (list :status process)
                                    events)
                                   'run))
                                ((symbol-function
                                  'kill-process)
                                 (lambda (process)
                                   (push
                                    (list :kill process)
                                    events)
                                   :killed))
                                ((symbol-function
                                  'start-process)
                                 (lambda
                                     (name buffer program &rest arguments)
                                   (push
                                    (list
                                     :start
                                     name
                                     buffer
                                     program
                                     arguments)
                                    events)
                                   'new-process))
                                ((symbol-function
                                  'set-process-query-on-exit-flag)
                                 (lambda (process flag)
                                   (push
                                    (list :query process flag)
                                    events)
                                   :query-disabled)))
                             (list
                              (anm/play-file file)
                              anm/process
                              (nreverse events)))))"##,
        expect![[
            r#"OK (:query-disabled new-process ((:status old-process) (:kill old-process) (:start "anm/player-command" fake-process-buffer "mock-player" ("--quiet" "[ORACLE-SANDBOX]/external-player/voice memo.wav" "--again" "[ORACLE-SANDBOX]/external-player/voice memo.wav")) (:query new-process nil)))"#
        ]],
    )
}

fn audio_notes_mode_external_player_preserves_stopped_process_until_new_process_is_started()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_external_player_preserves_stopped_process_until_new_process_is_started",
        r##"(progn
                          (require 'cl)
                          (let* ((directory
                                 (audio-notes-test-directory
                                  "stopped-player"))
                                (file
                                 (audio-notes-test-write
                                  directory
                                  "note.mp3"
                                  "audio"))
                                (anm/player-command
                                 '("player" file))
                                (anm/process-buffer nil)
                                (anm/process 'stopped-process)
                                events)
                           (cl-letf
                               (((symbol-function
                                  'process-status)
                                 (lambda (process)
                                   (push
                                    (list :status process)
                                    events)
                                   'exit))
                                ((symbol-function
                                  'kill-process)
                                 (lambda (process)
                                   (push
                                    (list :unexpected-kill process)
                                    events)))
                                ((symbol-function
                                  'start-process)
                                 (lambda (&rest arguments)
                                   (push
                                    (cons :start arguments)
                                    events)
                                   'replacement))
                                ((symbol-function
                                  'set-process-query-on-exit-flag)
                                 (lambda (&rest arguments)
                                   (push
                                    (cons :query arguments)
                                    events)
                                   :done)))
                             (list
                              (anm/play-file file)
                              anm/process
                              (nreverse events)))))"##,
        expect![[
            r#"OK (:done replacement ((:status stopped-process) (:start "anm/player-command" nil "player" "[ORACLE-SANDBOX]/stopped-player/note.mp3") (:query replacement nil)))"#
        ]],
    )
}

fn audio_notes_mode_external_player_surfaces_undeclared_legacy_cl_runtime_dependency()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_external_player_surfaces_undeclared_legacy_cl_runtime_dependency",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "missing-cl-runtime"))
                                (file
                                 (audio-notes-test-write
                                  directory
                                  "memo.wav"
                                  "audio"))
                                (anm/player-command
                                 '("mock-player" file))
                                (anm/process nil)
                                events)
                           (cl-letf
                               (((symbol-function
                                  'start-process)
                                 (lambda (&rest arguments)
                                   (push
                                    (cons :unexpected-start arguments)
                                    events)
                                   'process)))
                             (list
                              (fboundp 'concatenate)
                              (fboundp 'map)
                              (audio-notes-test-error
                               (lambda ()
                                 (anm/play-file file)))
                              anm/process
                              (nreverse events))))"##,
        expect!["OK (nil nil (:signal void-function (concatenate)) nil nil)"],
    )
    .fresh_process()
}

fn audio_notes_mode_internal_player_expands_real_file_and_returns_backend_value() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audio_notes_mode_internal_player_expands_real_file_and_returns_backend_value",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "internal-player"))
                                (file
                                 (audio-notes-test-write
                                  directory
                                  "relative.wav"
                                  "audio"))
                                (relative
                                 (file-relative-name
                                  file
                                  default-directory))
                                (anm/player-command
                                 'internal)
                                calls)
                           (cl-letf
                               (((symbol-function
                                  'play-sound-file)
                                 (lambda (path)
                                   (push path calls)
                                   :sound-finished)))
                             (list
                              (anm/play-file relative)
                              (nreverse calls))))"##,
        expect![[r#"OK (:sound-finished ("[ORACLE-SANDBOX]/internal-player/relative.wav"))"#]],
    )
}

fn audio_notes_mode_internal_unknown_format_disables_mode_and_signals_guidance() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audio_notes_mode_internal_unknown_format_disables_mode_and_signals_guidance",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "unknown-format"))
                                (file
                                 (audio-notes-test-write
                                  directory
                                  "memo.m4a"
                                  "audio"))
                                (anm/player-command
                                 'internal)
                                calls)
                           (cl-letf
                               (((symbol-function
                                  'play-sound-file)
                                 (lambda (_path)
                                   (error
                                    "Unknown sound format")))
                                ((symbol-function
                                  'audio-notes-mode)
                                 (lambda (&optional argument)
                                   (push
                                    (list :mode argument)
                                    calls)
                                   :disabled)))
                             (list
                              (audio-notes-test-error
                               (lambda ()
                                 (anm/play-file file)))
                              (nreverse calls))))"##,
        expect![[
            r#"OK ((:signal error ("Oops! Emacs internal player, can’t play the format of the file [ORACLE-SANDBOX]/unknown-format/memo.m4a.\nChange ‘anm/player’ to a command name (like \"mplayer\").")) ((:mode -1)))"#
        ]],
    )
}

fn audio_notes_mode_internal_arbitrary_backend_error_preserves_legacy_signal_shape()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_internal_arbitrary_backend_error_preserves_legacy_signal_shape",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "backend-error"))
                                (file
                                 (audio-notes-test-write
                                  directory
                                  "memo.wav"
                                  "audio"))
                                (anm/player-command
                                 'internal)
                                calls)
                           (cl-letf
                               (((symbol-function
                                  'play-sound-file)
                                 (lambda (_path)
                                   (signal
                                    'file-error
                                    '("decoder failed"
                                      17))))
                                ((symbol-function
                                  'audio-notes-mode)
                                 (lambda (&optional argument)
                                   (push
                                    (list :mode argument)
                                    calls)
                                   :disabled)))
                             (list
                              (audio-notes-test-error
                               (lambda ()
                                 (anm/play-file file)))
                              (nreverse calls))))"##,
        expect![[
            r#"OK ((:signal wrong-type-argument (stringp ("decoder failed" 17))) ((:mode -1)))"#
        ]],
    )
}

fn audio_notes_mode_play_file_rejects_missing_files_and_invalid_player_configurations()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_play_file_rejects_missing_files_and_invalid_player_configurations",
        r##"(let ((missing
                                (expand-file-name
                                 "missing.wav"
                                 default-directory))
                               calls)
                           (cl-letf
                               (((symbol-function
                                  'audio-notes-mode)
                                 (lambda (&optional argument)
                                   (push
                                    (list :mode argument)
                                    calls)
                                   :disabled)))
                             (let ((missing-result
                                    (audio-notes-test-error
                                     (lambda ()
                                       (let ((anm/player-command
                                              'internal))
                                         (anm/play-file
                                          missing))))))
                               (let* ((directory
                                       (audio-notes-test-directory
                                        "invalid-player"))
                                      (file
                                       (audio-notes-test-write
                                        directory
                                        "memo.wav"
                                        "audio"))
                                      (configs
                                       '(nil
                                         invalid
                                         42
                                         "player"))
                                      (invalid-results
                                       (mapcar
                                        (lambda (config)
                                          (list
                                           config
                                           (audio-notes-test-error
                                            (lambda ()
                                              (let ((anm/player-command
                                                     config))
                                                (anm/play-file
                                                 file))))))
                                        configs)))
                                 (list
                                  missing-result
                                  invalid-results
                                  (nreverse calls))))))"##,
        expect![[
            r#"OK ((:signal error ("FILE isn’t a file!")) ((nil (:signal void-function (concatenate))) (invalid (:signal error ("‘anm/player-command’ invalid: invalid"))) (42 (:signal error ("‘anm/player-command’ invalid: 42"))) ("player" (:signal error ("‘anm/player-command’ invalid: player")))) ((:mode -1)))"#
        ]],
    )
    .fresh_process()
}

fn audio_notes_mode_first_note_workflow_selects_file_updates_buffers_and_runs_hooks_in_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_first_note_workflow_selects_file_updates_buffers_and_runs_hooks_in_order",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "first-note"))
                                (first
                                 (audio-notes-test-write
                                  directory
                                  "01-first.wav"
                                  "first"))
                                (second
                                 (audio-notes-test-write
                                  directory
                                  "02-second.wav"
                                  "second"))
                                (anm/current nil)
                                (anm/process nil)
                                (anm/dired-buffer
                                 (generate-new-buffer
                                  " *audio-notes-dired*"))
                                (anm/process-buffer
                                 (generate-new-buffer
                                  " *audio-notes-process*"))
                                events)
                           (unwind-protect
                               (progn
                                 (with-current-buffer
                                     anm/dired-buffer
                                   (insert
                                    "header\n01-first.wav\n02-second.wav\n"))
                                 (with-current-buffer
                                     anm/process-buffer
                                   (insert
                                    "stale process output"))
                                 (let ((anm/before-play-hook
                                        (list
                                         (lambda ()
                                           (push
                                            (list :before anm/current)
                                            events))))
                                       (anm/after-play-hook
                                        (list
                                         (lambda ()
                                           (push
                                            (list :after anm/current)
                                            events)))))
                                   (cl-letf
                                       (((symbol-function
                                          'anm/list-files)
                                         (lambda ()
                                           (push :list-files events)
                                           (list first second)))
                                        ((symbol-function
                                          'revert-buffer)
                                         (lambda (&rest arguments)
                                           (push
                                            (list :revert arguments)
                                            events)
                                           :reverted))
                                        ((symbol-function
                                          'anm/play-file)
                                         (lambda (file)
                                           (push
                                            (list :play-file file)
                                            events)
                                           :playing))
                                        ((symbol-function
                                          'message)
                                         (lambda
                                             (format-string &rest arguments)
                                           (let ((text
                                                  (apply
                                                   #'format
                                                   format-string
                                                   arguments)))
                                             (push
                                              (list :message text)
                                              events)
                                             text))))
                                     (let ((result
                                            (anm/play-pause-current)))
                                       (list
                                        result
                                        (file-name-nondirectory
                                         anm/current)
                                        (with-current-buffer
                                            anm/dired-buffer
                                          (list
                                           (point)
                                           (buffer-substring-no-properties
                                            (line-beginning-position)
                                            (line-end-position))))
                                        (with-current-buffer
                                            anm/process-buffer
                                          (buffer-string))
                                        (nreverse events))))))
                             (when
                                 (buffer-live-p anm/dired-buffer)
                               (kill-buffer anm/dired-buffer))
                             (when
                                 (buffer-live-p anm/process-buffer)
                               (kill-buffer anm/process-buffer))))"##,
        expect![[
            r#"OK (nil "01-first.wav" (20 "01-first.wav") "" (:list-files (:message "2 notes left. Playing 01-first.wav") (:revert nil) (:before "[ORACLE-SANDBOX]/first-note/01-first.wav") (:play-file "[ORACLE-SANDBOX]/first-note/01-first.wav") (:after "[ORACLE-SANDBOX]/first-note/01-first.wav")))"#
        ]],
    )
}

fn audio_notes_mode_replay_workflow_keeps_current_note_and_reports_replay() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_replay_workflow_keeps_current_note_and_reports_replay",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "replay"))
                                (current
                                 (audio-notes-test-write
                                  directory
                                  "current.mp3"
                                  "audio"))
                                (anm/current current)
                                (anm/process nil)
                                (anm/dired-buffer
                                 (generate-new-buffer
                                  " *audio-notes-replay-dired*"))
                                (anm/process-buffer
                                 (generate-new-buffer
                                  " *audio-notes-replay-process*"))
                                events)
                           (unwind-protect
                               (progn
                                 (with-current-buffer
                                     anm/dired-buffer
                                   (insert
                                    "current.mp3\n"))
                                 (cl-letf
                                     (((symbol-function
                                        'anm/-is-alive-p)
                                       (lambda () nil))
                                      ((symbol-function
                                        'anm/list-files)
                                       (lambda ()
                                         (push :list-files events)
                                         nil))
                                      ((symbol-function
                                        'revert-buffer)
                                       (lambda (&rest _arguments)
                                         (push :revert events)))
                                      ((symbol-function
                                        'anm/play-file)
                                       (lambda (file)
                                         (push
                                          (list :play file)
                                          events)
                                         :replayed))
                                      ((symbol-function
                                        'message)
                                       (lambda
                                           (format-string &rest arguments)
                                         (let ((text
                                                (apply
                                                 #'format
                                                 format-string
                                                 arguments)))
                                           (push
                                            (list :message text)
                                            events)
                                           text))))
                                   (list
                                    (anm/play-pause-current)
                                    (eq anm/current current)
                                    (nreverse events))))
                             (when
                                 (buffer-live-p anm/dired-buffer)
                               (kill-buffer anm/dired-buffer))
                             (when
                                 (buffer-live-p anm/process-buffer)
                               (kill-buffer anm/process-buffer))))"##,
        expect![[
            r#"OK (nil t (:list-files (:message "Replaying current.mp3") :revert (:play "[ORACLE-SANDBOX]/replay/current.mp3")))"#
        ]],
    )
}

fn audio_notes_mode_live_playback_toggles_mplayer_pause_or_stops_other_players() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audio_notes_mode_live_playback_toggles_mplayer_pause_or_stops_other_players",
        r##"(let ((anm/current
                                "/fixed/current.wav")
                               events
                               mplayer)
                           (cl-letf
                               (((symbol-function
                                  'anm/-is-alive-p)
                                 (lambda ()
                                   (push :alive events)
                                   t))
                                ((symbol-function
                                  'anm/-is-mplayer-p)
                                 (lambda ()
                                   (push
                                    (list :mplayer mplayer)
                                    events)
                                   mplayer))
                                ((symbol-function
                                  'anm/-mplayer-send)
                                 (lambda (command)
                                   (push
                                    (list :send command)
                                    events)
                                   :paused))
                                ((symbol-function
                                  'anm/stop)
                                 (lambda ()
                                   (push :stop events)
                                   :stopped))
                                ((symbol-function
                                  'anm/list-files)
                                 (lambda ()
                                   (push :unexpected-list events)
                                   nil)))
                             (setq mplayer t)
                             (let ((pause-result
                                    (anm/play-pause-current)))
                               (setq mplayer nil)
                               (let ((stop-result
                                      (anm/play-pause-current)))
                                 (list
                                  pause-result
                                  stop-result
                                  (nreverse events))))))"##,
        expect![[
            r#"OK (:paused :stopped (:alive (:mplayer t) (:send "pause") :alive (:mplayer nil) :stop))"#
        ]],
    )
}

fn audio_notes_mode_empty_queue_exits_mode_without_touching_player_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_empty_queue_exits_mode_without_touching_player_buffers",
        r##"(let ((anm/current nil)
                               events)
                           (cl-letf
                               (((symbol-function
                                  'anm/list-files)
                                 (lambda ()
                                   (push :list events)
                                   nil))
                                ((symbol-function
                                  'audio-notes-mode)
                                 (lambda (&optional argument)
                                   (push
                                    (list :mode argument)
                                    events)
                                   :disabled))
                                ((symbol-function
                                  'message)
                                 (lambda
                                     (format-string &rest arguments)
                                   (let ((text
                                          (apply
                                           #'format
                                           format-string
                                           arguments)))
                                     (push
                                      (list :message text)
                                      events)
                                     text))))
                             (list
                              (anm/play-pause-current)
                              anm/current
                              (nreverse events))))"##,
        expect![[
            r#"OK (:disabled nil (:list (:message "No more notes. Exiting `audio-notes-mode'.") (:mode -1)))"#
        ]],
    )
}

fn audio_notes_mode_before_hook_error_aborts_player_and_after_hook_execution() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_before_hook_error_aborts_player_and_after_hook_execution",
        r##"(let* ((directory
                                 (audio-notes-test-directory
                                  "hook-error"))
                                (file
                                 (audio-notes-test-write
                                  directory
                                  "memo.wav"
                                  "audio"))
                                (anm/current nil)
                                (anm/dired-buffer
                                 (generate-new-buffer
                                  " *audio-notes-hook-dired*"))
                                (anm/process-buffer
                                 (generate-new-buffer
                                  " *audio-notes-hook-process*"))
                                events)
                           (unwind-protect
                               (progn
                                 (with-current-buffer
                                     anm/dired-buffer
                                   (insert "memo.wav\n"))
                                 (let ((anm/before-play-hook
                                        (list
                                         (lambda ()
                                           (push :before events)
                                           (error
                                            "before hook failed"))))
                                       (anm/after-play-hook
                                        (list
                                         (lambda ()
                                           (push :after events)))))
                                   (cl-letf
                                       (((symbol-function
                                          'anm/list-files)
                                         (lambda ()
                                           (list file)))
                                        ((symbol-function
                                          'revert-buffer)
                                         (lambda (&rest _arguments)
                                           :reverted))
                                        ((symbol-function
                                          'anm/play-file)
                                         (lambda (_file)
                                           (push :play events)
                                           :playing))
                                        ((symbol-function
                                          'message)
                                         (lambda (&rest _arguments)
                                           nil)))
                                     (list
                                      (audio-notes-test-error
                                       (lambda ()
                                         (anm/play-pause-current)))
                                      (file-name-nondirectory
                                       anm/current)
                                      (nreverse events)))))
                             (when
                                 (buffer-live-p anm/dired-buffer)
                               (kill-buffer anm/dired-buffer))
                             (when
                                 (buffer-live-p anm/process-buffer)
                               (kill-buffer anm/process-buffer))))"##,
        expect![[r#"OK ((:signal error ("before hook failed")) "memo.wav" (:before))"#]],
    )
}

pub(super) fn playback_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audio_notes_mode_external_player_expands_file_arguments_replaces_live_process_and_disables_exit_query(),
        audio_notes_mode_external_player_preserves_stopped_process_until_new_process_is_started(),
        audio_notes_mode_external_player_surfaces_undeclared_legacy_cl_runtime_dependency(),
        audio_notes_mode_internal_player_expands_real_file_and_returns_backend_value(),
        audio_notes_mode_internal_unknown_format_disables_mode_and_signals_guidance(),
        audio_notes_mode_internal_arbitrary_backend_error_preserves_legacy_signal_shape(),
        audio_notes_mode_play_file_rejects_missing_files_and_invalid_player_configurations(),
        audio_notes_mode_first_note_workflow_selects_file_updates_buffers_and_runs_hooks_in_order(),
        audio_notes_mode_replay_workflow_keeps_current_note_and_reports_replay(),
        audio_notes_mode_live_playback_toggles_mplayer_pause_or_stops_other_players(),
        audio_notes_mode_empty_queue_exits_mode_without_touching_player_buffers(),
        audio_notes_mode_before_hook_error_aborts_player_and_after_hook_execution(),
    ]
}
