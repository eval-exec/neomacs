use expect_test::expect;

use super::ParityBatchCase;

fn audio_notes_mode_mplayer_prefix_parser_handles_numeric_raw_and_error_inputs() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audio_notes_mode_mplayer_prefix_parser_handles_numeric_raw_and_error_inputs",
        r##"(let ((anm/default-seek-step 5))
                           (mapcar
                            (lambda (input)
                              (list
                               input
                               (audio-notes-test-error
                                (lambda ()
                                  (anm/-mplayer-parse-seconds
                                   input)))))
                            '(nil
                              0
                              3
                              -7
                              2.5
                              (1)
                              (4)
                              (16)
                              (-16)
                              (64)
                              ()
                              (0)
                              symbol
                              "5")))"##,
        expect![[
            r#"OK ((nil (:ok 5)) (0 (:ok 0)) (3 (:ok 3)) (-7 (:ok -7)) (2.5 (:ok 2.5)) ((1) (:ok 5.0)) ((4) (:ok 10.0)) ((16) (:ok 15.0)) ((-16) (:ok 15.0)) ((64) (:ok 20.0)) (nil (:ok 5)) ((0) (:ok -1.0e+INF)) (symbol (:ok nil)) ("5" (:ok nil)))"#
        ]],
    )
}

fn audio_notes_mode_mplayer_prefix_parser_scales_with_custom_default_step() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_mplayer_prefix_parser_scales_with_custom_default_step",
        r##"(mapcar
                          (lambda (step)
                            (let ((anm/default-seek-step step))
                              (list
                               step
                               (anm/-mplayer-parse-seconds nil)
                               (anm/-mplayer-parse-seconds 7)
                               (anm/-mplayer-parse-seconds '(1))
                               (anm/-mplayer-parse-seconds '(4))
                               (anm/-mplayer-parse-seconds '(16)))))
                          '(1 3 10 -2 0))"##,
        expect![
            "OK ((1 1 7 1.0 2.0 3.0) (3 3 7 3.0 6.0 9.0) (10 10 7 10.0 20.0 30.0) (-2 -2 7 -2.0 -4.0 -6.0) (0 0 7 0.0 0.0 0.0))"
        ],
    )
}

fn audio_notes_mode_mplayer_and_process_alive_predicate_matrix_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_mplayer_and_process_alive_predicate_matrix_matches",
        r##"(let (status-calls)
                           (cl-letf
                               (((symbol-function
                                  'process-status)
                                 (lambda (process)
                                   (push process status-calls)
                                   (cdr
                                    (assq
                                     process
                                     '((running . run)
                                       (stopped . stop)
                                       (exited . exit)))))))
                             (list
                              (mapcar
                               (lambda (command)
                                 (let ((anm/player-command command))
                                   (list
                                    command
                                    (audio-notes-test-error
                                     (lambda ()
                                       (anm/-is-mplayer-p))))))
                               '(internal
                                 nil
                                 ()
                                 ("mplayer")
                                 ("mplayer" "-quiet" file)
                                 ("vlc" file)
                                 (mplayer file)
                                 ("MPLAYER" file)
                                 ("mplayer" . file)))
                              (mapcar
                               (lambda (process)
                                 (let ((anm/process process))
                                   (list
                                    process
                                    (anm/-is-alive-p))))
                               '(nil
                                 running
                                 stopped
                                 exited))
                              (nreverse status-calls))))"##,
        expect![[
            r#"OK (((internal (:ok nil)) (nil (:ok nil)) (nil (:ok nil)) (("mplayer") (:ok t)) (("mplayer" "-quiet" file) (:ok t)) (("vlc" file) (:ok nil)) ((mplayer file) (:ok t)) (("MPLAYER" file) (:ok nil)) (("mplayer" . file) (:ok t))) ((nil nil) (running t) (stopped nil) (exited nil)) (running stopped exited))"#
        ]],
    )
}

fn audio_notes_mode_mplayer_send_routes_commands_and_exact_failure_messages() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_mplayer_send_routes_commands_and_exact_failure_messages",
        r##"(let (events
                               using-mplayer
                               alive)
                           (cl-letf
                               (((symbol-function
                                  'anm/-is-mplayer-p)
                                 (lambda ()
                                   (push
                                    (list :is-mplayer using-mplayer)
                                    events)
                                   using-mplayer))
                                ((symbol-function
                                  'anm/-is-alive-p)
                                 (lambda ()
                                   (push
                                    (list :is-alive alive)
                                    events)
                                   alive))
                                ((symbol-function
                                  'process-send-string)
                                 (lambda (process string)
                                   (push
                                    (list :send process string)
                                    events)
                                   :sent))
                                ((symbol-function
                                  'message)
                                 (lambda (format-string &rest arguments)
                                   (let ((text
                                          (apply
                                           #'format
                                           format-string
                                           arguments)))
                                     (push
                                      (list :message text)
                                      events)
                                     text))))
                             (let ((anm/process 'fake-process))
                               (setq using-mplayer t
                                     alive t)
                               (let ((sent
                                      (anm/-mplayer-send
                                       "seek 15 0")))
                                 (setq using-mplayer t
                                       alive nil)
                                 (let ((dead
                                        (anm/-mplayer-send
                                         "pause")))
                                   (setq using-mplayer nil
                                         alive t)
                                   (let ((other
                                          (anm/-mplayer-send
                                           "pause")))
                                     (list
                                      sent
                                      dead
                                      other
                                      (nreverse events))))))))"##,
        expect![[
            r#"OK (:sent "There's nothing playing!" "Not using mplayer!" ((:is-mplayer t) (:is-alive t) (:send fake-process "seek 15 0\n") (:is-mplayer t) (:is-alive nil) (:message "There's nothing playing!") (:is-mplayer nil) (:message "Not using mplayer!")))"#
        ]],
    )
}

fn audio_notes_mode_seek_commands_translate_direct_and_raw_prefixes_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_seek_commands_translate_direct_and_raw_prefixes_exactly",
        r##"(let ((anm/default-seek-step 5)
                               commands)
                           (cl-letf
                               (((symbol-function
                                  'anm/-mplayer-send)
                                 (lambda (command)
                                   (push command commands)
                                   (concat "sent:" command))))
                             (let ((results
                                    (list
                                     (anm/mplayer-seek-forward nil)
                                     (anm/mplayer-seek-forward 3)
                                     (anm/mplayer-seek-forward -2)
                                     (anm/mplayer-seek-forward '(4))
                                     (anm/mplayer-seek-backward nil)
                                     (anm/mplayer-seek-backward 3)
                                     (anm/mplayer-seek-backward -2)
                                     (anm/mplayer-seek-backward '(16)))))
                               (list
                                results
                                (nreverse commands)))))"##,
        expect![[
            r#"OK (("sent:seek 5 0" "sent:seek 3 0" "sent:seek -2 0" "sent:seek 10 0" "sent:seek -5 0" "sent:seek -3 0" "sent:seek 2 0" "sent:seek -15 0") ("seek 5 0" "seek 3 0" "seek -2 0" "seek 10 0" "seek -5 0" "seek -3 0" "seek 2 0" "seek -15 0"))"#
        ]],
    )
}

fn audio_notes_mode_seek_commands_honor_interactive_prefix_protocol() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_seek_commands_honor_interactive_prefix_protocol",
        r##"(let ((anm/default-seek-step 4)
                               commands)
                           (cl-letf
                               (((symbol-function
                                  'anm/-mplayer-send)
                                 (lambda (command)
                                   (push command commands)
                                   command)))
                             (dolist
                                 (entry
                                  '((anm/mplayer-seek-forward nil)
                                    (anm/mplayer-seek-forward 3)
                                    (anm/mplayer-seek-forward (4))
                                    (anm/mplayer-seek-backward nil)
                                    (anm/mplayer-seek-backward -2)
                                    (anm/mplayer-seek-backward (16))))
                               (let ((current-prefix-arg
                                      (cadr entry)))
                                 (call-interactively
                                  (car entry))))
                             (nreverse commands)))"##,
        expect![[r#"OK ("seek 4 0" "seek 3 0" "seek 8 0" "seek -4 0" "seek 2 0" "seek -12 0")"#]],
    )
}

fn audio_notes_mode_stop_kills_live_player_or_reports_exact_idle_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_stop_kills_live_player_or_reports_exact_idle_message",
        r##"(let (events alive)
                           (cl-letf
                               (((symbol-function
                                  'anm/-is-alive-p)
                                 (lambda ()
                                   (push
                                    (list :alive alive)
                                    events)
                                   alive))
                                ((symbol-function
                                  'kill-process)
                                 (lambda (process)
                                   (push
                                    (list :kill process)
                                    events)
                                   :killed))
                                ((symbol-function
                                  'message)
                                 (lambda (format-string &rest arguments)
                                   (let ((text
                                          (apply
                                           #'format
                                           format-string
                                           arguments)))
                                     (push
                                      (list :message text)
                                      events)
                                     text))))
                             (let ((anm/process 'fake-player))
                               (setq alive t)
                               (let ((live-result
                                      (anm/stop)))
                                 (setq alive nil)
                                 (let ((idle-result
                                        (anm/stop)))
                                   (list
                                    live-result
                                    idle-result
                                    (nreverse events)))))))"##,
        expect![[
            r#"OK (:killed "There's nothing playing!" ((:alive t) (:kill fake-player) (:alive nil) (:message "There's nothing playing!")))"#
        ]],
    )
}

fn audio_notes_mode_bug_report_opens_exact_url_and_formats_version_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_bug_report_opens_exact_url_and_formats_version_message",
        r##"(let ((emacs-version
                                "99.88-test")
                               events)
                           (cl-letf
                               (((symbol-function
                                  'browse-url)
                                 (lambda (url &rest arguments)
                                   (push
                                    (list :browse url arguments)
                                    events)
                                   :opened))
                                ((symbol-function
                                  'message)
                                 (lambda (format-string &rest arguments)
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
                              (anm/bug-report)
                              (nreverse events))))"##,
        expect![[
            r#"OK ("Your anm/version is: 1.1.1, and your emacs version is: 99.88-test.\nPlease include this in your report!" ((:browse "https://github.com/Bruce-Connor/audio-notes-mode/issues/new" nil) (:message "Your anm/version is: 1.1.1, and your emacs version is: 99.88-test.\nPlease include this in your report!")))"#
        ]],
    )
}

fn audio_notes_mode_customize_opens_exact_group_with_other_window_flag() -> ParityBatchCase {
    ParityBatchCase::value(
        "audio_notes_mode_customize_opens_exact_group_with_other_window_flag",
        r##"(let (calls)
                           (cl-letf
                               (((symbol-function
                                  'customize-group)
                                 (lambda
                                     (group &optional other-window)
                                   (push
                                    (list group other-window)
                                    calls)
                                   :customized)))
                             (list
                              (anm/customize)
                              (nreverse calls))))"##,
        expect!["OK (:customized ((audio-notes-mode t)))"],
    )
}

pub(super) fn process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audio_notes_mode_mplayer_prefix_parser_handles_numeric_raw_and_error_inputs(),
        audio_notes_mode_mplayer_prefix_parser_scales_with_custom_default_step(),
        audio_notes_mode_mplayer_and_process_alive_predicate_matrix_matches(),
        audio_notes_mode_mplayer_send_routes_commands_and_exact_failure_messages(),
        audio_notes_mode_seek_commands_translate_direct_and_raw_prefixes_exactly(),
        audio_notes_mode_seek_commands_honor_interactive_prefix_protocol(),
        audio_notes_mode_stop_kills_live_player_or_reports_exact_idle_message(),
        audio_notes_mode_bug_report_opens_exact_url_and_formats_version_message(),
        audio_notes_mode_customize_opens_exact_group_with_other_window_flag(),
    ]
}
