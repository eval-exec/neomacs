use expect_test::expect;

use super::ParityBatchCase;

fn audacious_run_and_confirmed_kill_issue_exact_process_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_run_and_confirmed_kill_issue_exact_process_contracts",
        r##"(mapcar
         (lambda (answer)
           (let (calls prompts)
             (cl-letf
                 (((symbol-function 'call-process)
                   (lambda (&rest arguments)
                     (push arguments calls)
                     (list
                      :process-return
                      (car arguments))))
                  ((symbol-function 'yes-or-no-p)
                   (lambda (prompt)
                     (push prompt prompts)
                     answer)))
               (list
                answer
                (audacious-run)
                (audacious-kill)
                (nreverse prompts)
                (nreverse calls)))))
         '(nil t))"##,
        expect![[
            r#"OK ((nil (:process-return "audacious") nil ("Quit Audacious ?") (("audacious" nil 0 nil "-H" "2>/dev/null"))) (t (:process-return "audacious") (:process-return "/fixture/bin/audtool") ("Quit Audacious ?") (("audacious" nil 0 nil "-H" "2>/dev/null") ("/fixture/bin/audtool" nil 0 nil "--shutdown"))))"#
        ]],
    )
}

fn audacious_manual_volume_forwards_practical_and_edge_values_without_validation() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audacious_manual_volume_forwards_practical_and_edge_values_without_validation",
        r##"(let (calls)
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push arguments calls)
                 (length arguments))))
           (list
            (mapcar
             #'audacious-volume
             '("+25%"
               "-0%"
               "100"
               ""
               :symbolic))
            (nreverse calls))))"##,
        expect![[
            r#"OK ((6 6 6 6 6) (("/fixture/bin/audtool" nil nil nil "--set-volume" "+25%") ("/fixture/bin/audtool" nil nil nil "--set-volume" "-0%") ("/fixture/bin/audtool" nil nil nil "--set-volume" "100") ("/fixture/bin/audtool" nil nil nil "--set-volume" "") ("/fixture/bin/audtool" nil nil nil "--set-volume" :symbolic)))"#
        ]],
    )
}

fn audacious_volume_shortcuts_set_exact_delta_then_report_trimmed_live_volume() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_volume_shortcuts_set_exact_delta_then_report_trimmed_live_volume",
        r##"(let (events)
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :call arguments)
                  events)
                 0))
              ((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 "  73% \n"))
              ((symbol-function 'message)
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
            (audacious-volume-up)
            (audacious-volume-down)
            (nreverse events))))"##,
        expect![[
            r#"OK ("73%" "73%" ((:call "/fixture/bin/audtool" nil nil nil "--set-volume" "+10%") (:shell "audtool --get-volume") (:message "73%") (:call "/fixture/bin/audtool" nil nil nil "--set-volume" "-10%") (:shell "audtool --get-volume") (:message "73%")))"#
        ]],
    )
}

fn audacious_pause_status_and_stop_preserve_command_query_and_message_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_pause_status_and_stop_preserve_command_query_and_message_order",
        r##"(let (events)
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :call arguments)
                  events)
                 :called))
              ((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 "  paused \n"))
              ((symbol-function 'message)
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
            (audacious-pause)
            (audacious-status)
            (audacious-stop)
            (nreverse events))))"##,
        expect![[
            r#"OK ("paused" "paused" :called ((:call "/fixture/bin/audtool" nil nil nil "--playback-pause") (:shell "audtool --playback-status") (:message "paused") (:shell "audtool --playback-status") (:message "paused") (:call "/fixture/bin/audtool" nil nil nil "--playback-stop")))"#
        ]],
    )
}

fn audacious_random_toggle_reports_next_state_before_toggling_for_off_and_on() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_random_toggle_reports_next_state_before_toggling_for_off_and_on",
        r##"(let ((statuses
                '("shuffle off\n"
                  "shuffle on\n"))
               events)
         (cl-letf
             (((symbol-function 'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 (pop statuses)))
              ((symbol-function 'message)
               (lambda (text)
                 (push
                  (list :message text)
                  events)
                 text))
              ((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :call arguments)
                  events)
                 :toggled)))
           (list
            (audacious-random-toggle)
            (audacious-random-toggle)
            (nreverse events))))"##,
        expect![[
            r#"OK (:toggled :toggled ((:shell "audtool --playlist-shuffle-status") (:message "Random: ON") (:call "/fixture/bin/audtool" nil nil nil "--playlist-shuffle-toggle") (:shell "audtool --playlist-shuffle-status") (:message "Random: OFF") (:call "/fixture/bin/audtool" nil nil nil "--playlist-shuffle-toggle")))"#
        ]],
    )
}

fn audacious_repeat_toggle_uses_substring_status_semantics_and_exact_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_repeat_toggle_uses_substring_status_semantics_and_exact_command",
        r##"(let ((statuses
                '("not-official\n"
                  "enabled\n"
                  "OFF\n"))
               events)
         (cl-letf
             (((symbol-function 'shell-command-to-string)
               (lambda (_command)
                 (pop statuses)))
              ((symbol-function 'message)
               (lambda (text)
                 (push text events)
                 text))
              ((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push arguments events)
                 0)))
           (list
            (audacious-repeat-toggle)
            (audacious-repeat-toggle)
            (audacious-repeat-toggle)
            (nreverse events))))"##,
        expect![[
            r#"OK (0 0 0 ("Repeat: ON" ("/fixture/bin/audtool" nil nil nil "--playlist-repeat-toggle") "Repeat: OFF" ("/fixture/bin/audtool" nil nil nil "--playlist-repeat-toggle") "Repeat: ON" ("/fixture/bin/audtool" nil nil nil "--playlist-repeat-toggle")))"#
        ]],
    )
}

fn audacious_seek_commands_forward_offsets_and_refresh_only_shortcuts() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_seek_commands_forward_offsets_and_refresh_only_shortcuts",
        r##"(let (events)
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :call arguments)
                  events)
                 :seeked))
              ((symbol-function
                'audacious-song-show-current-info)
               (lambda ()
                 (push :refresh events)
                 :refreshed)))
           (list
            (audacious-song-seek "+3.75")
            (audacious-song-seek-backward)
            (audacious-song-seek-forward)
            (nreverse events))))"##,
        expect![[
            r#"OK (:seeked :refreshed :refreshed ((:call "/fixture/bin/audtool" nil nil nil "--playback-seek-relative" "+3.75") (:call "/fixture/bin/audtool" nil nil nil "--playback-seek-relative" "-10") :refresh (:call "/fixture/bin/audtool" nil nil nil "--playback-seek-relative" "+10") :refresh))"#
        ]],
    )
}

fn audacious_song_navigation_advances_or_reverses_then_waits_and_refreshes() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_song_navigation_advances_or_reverses_then_waits_and_refreshes",
        r##"(let (events)
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :call arguments)
                  events)
                 0))
              ((symbol-function 'sleep-for)
               (lambda (&rest arguments)
                 (push
                  (cons :sleep arguments)
                  events)
                 :slept))
              ((symbol-function
                'audacious-song-show-current-info)
               (lambda ()
                 (push :refresh events)
                 :refreshed)))
           (list
            (audacious-song-next)
            (audacious-song-prev)
            (nreverse events))))"##,
        expect![[
            r#"OK (:refreshed :refreshed ((:call "/fixture/bin/audtool" nil nil nil "--playlist-advance") (:sleep 0 20) :refresh (:call "/fixture/bin/audtool" nil nil nil "--playlist-reverse") (:sleep 0 20) :refresh))"#
        ]],
    )
}

fn audacious_play_starts_daemon_only_for_exact_empty_status_then_plays_and_refreshes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_play_starts_daemon_only_for_exact_empty_status_then_plays_and_refreshes",
        r##"(mapcar
         (lambda (status)
           (let (events)
             (cl-letf
                 (((symbol-function
                    'shell-command-to-string)
                   (lambda (command)
                     (push
                      (list :shell command)
                      events)
                     status))
                  ((symbol-function 'audacious-run)
                   (lambda ()
                     (push :run events)
                     :started))
                  ((symbol-function 'call-process)
                   (lambda (&rest arguments)
                     (push
                      (cons :call arguments)
                      events)
                     0))
                  ((symbol-function 'sleep-for)
                   (lambda (&rest arguments)
                     (push
                      (cons :sleep arguments)
                      events)))
                  ((symbol-function
                    'audacious-song-show-current-info)
                   (lambda ()
                     (push :refresh events)
                     :refreshed)))
               (list
                status
                (audacious-play)
                (nreverse events)))))
         '(""
           "stopped\n"
           " \n"))"##,
        expect![[
            r#"OK (("" :refreshed ((:shell "audtool --playback-status") :run (:call "/fixture/bin/audtool" nil nil nil "--playback-play") (:sleep 0 20) :refresh)) ("stopped\n" :refreshed ((:shell "audtool --playback-status") (:call "/fixture/bin/audtool" nil nil nil "--playback-play") (:sleep 0 20) :refresh)) (" \n" :refreshed ((:shell "audtool --playback-status") (:call "/fixture/bin/audtool" nil nil nil "--playback-play") (:sleep 0 20) :refresh)))"#
        ]],
    )
}

pub(super) fn commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audacious_run_and_confirmed_kill_issue_exact_process_contracts(),
        audacious_manual_volume_forwards_practical_and_edge_values_without_validation(),
        audacious_volume_shortcuts_set_exact_delta_then_report_trimmed_live_volume(),
        audacious_pause_status_and_stop_preserve_command_query_and_message_order(),
        audacious_random_toggle_reports_next_state_before_toggling_for_off_and_on(),
        audacious_repeat_toggle_uses_substring_status_semantics_and_exact_command(),
        audacious_seek_commands_forward_offsets_and_refresh_only_shortcuts(),
        audacious_song_navigation_advances_or_reverses_then_waits_and_refreshes(),
        audacious_play_starts_daemon_only_for_exact_empty_status_then_plays_and_refreshes(),
    ]
}
