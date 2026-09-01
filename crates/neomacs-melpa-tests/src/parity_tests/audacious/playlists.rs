use expect_test::expect;

use super::ParityBatchCase;

fn audacious_playlist_filters_only_pipe_rows_and_resets_prior_message_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_playlist_filters_only_pipe_rows_and_resets_prior_message_state",
        r##"(let ((audacious-msg
                "stale-data")
               messages)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (list
                  command)
                 "Header\n 1 | First\nplain title\n 2 || Second\nFooter\n"))
              ((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (let ((text
                        (apply
                         #'format
                         format-string
                         arguments)))
                   (push text messages)
                   text))))
           (list
            (audacious-playlist)
            audacious-msg
            (nreverse messages))))"##,
        expect![[
            r#"OK (" 1 | First\n 2 || Second\n" " 1 | First\n 2 || Second\n" (" 1 | First\n 2 || Second\n"))"#
        ]],
    )
}

fn audacious_playlist_current_info_trims_queries_updates_state_and_formats_message()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_playlist_current_info_trims_queries_updates_state_and_formats_message",
        r##"(let (events)
         (audacious-test-reset-state)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push command events)
                 (pcase command
                   ("audtool --current-playlist-name"
                    "  Focus Mix \n")
                   ("audtool --current-playlist"
                    " 2\n")
                   ("audtool --number-of-playlists"
                    "5 \n"))))
              ((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (let ((text
                        (apply
                         #'format
                         format-string
                         arguments)))
                   (push text events)
                   text))))
           (list
            (audacious-playlist-show-current-info)
            (list
             audacious-playlist-position
             audacious-playlist-length
             audacious-playlist-name)
            (nreverse events))))"##,
        expect![[
            r#"OK ("[2/5] \"Focus Mix\"" ("2" "5" "Focus Mix") ("audtool --current-playlist-name" "audtool --current-playlist" "audtool --number-of-playlists" "[2/5] \"Focus Mix\""))"#
        ]],
    )
}

fn audacious_private_playlist_goto_switches_waits_plays_and_refreshes_in_order() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audacious_private_playlist_goto_switches_waits_plays_and_refreshes_in_order",
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
                'audacious-playlist-show-current-info)
               (lambda ()
                 (push :refresh events)
                 :refreshed)))
           (list
            (audacious-playlist--goto "+02")
            (nreverse events))))"##,
        expect![[
            r#"OK (:refreshed ((:call "/fixture/bin/audtool" nil nil nil "--set-current-playlist" "+02") (:sleep 0 20) (:call "/fixture/bin/audtool" nil nil nil "--play-current-playlist") :refresh))"#
        ]],
    )
}

fn audacious_public_playlist_goto_reads_live_length_and_executes_numeric_selection()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_public_playlist_goto_reads_live_length_and_executes_numeric_selection",
        r##"(let (events prompts)
         (audacious-test-reset-state)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 " 4\n"))
              ((symbol-function 'read-string)
               (lambda (prompt)
                 (push prompt prompts)
                 "003"))
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
                'audacious-playlist-show-current-info)
               (lambda ()
                 (push :refresh events)
                 :refreshed)))
           (list
            (audacious-playlist-goto)
            audacious-playlist-position
            audacious-playlist-length
            (nreverse prompts)
            (nreverse events))))"##,
        expect![[
            r#"OK (:refreshed "003" "4" ("Playlist No. [1 - 4]: ") ((:shell "audtool --number-of-playlists") (:call "/fixture/bin/audtool" nil nil nil "--set-current-playlist" "003") (:sleep 0 20) (:call "/fixture/bin/audtool" nil nil nil "--play-current-playlist") :refresh))"#
        ]],
    )
}

fn audacious_public_playlist_goto_invalid_value_reports_without_process_or_sleep() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audacious_public_playlist_goto_invalid_value_reports_without_process_or_sleep",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (_command)
                 "9\n"))
              ((symbol-function 'read-string)
               (lambda (prompt)
                 (push
                  (list :prompt prompt)
                  events)
                 "two"))
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
                   text)))
              ((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :unexpected-call arguments)
                  events))))
           (list
            (audacious-playlist-goto)
            audacious-playlist-position
            audacious-playlist-length
            (nreverse events))))"##,
        expect![[
            r#"OK ("\"two\" is not number." "two" "9" ((:prompt "Playlist No. [1 - 9]: ") (:message "\"two\" is not number.")))"#
        ]],
    )
}

fn audacious_playlist_next_and_prev_boundaries_report_without_switching() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_playlist_next_and_prev_boundaries_report_without_switching",
        r##"(mapcar
         (lambda (case)
           (pcase-let
               ((`(,direction
                   ,position
                   ,length)
                 case))
             (let (events)
               (cl-letf
                   (((symbol-function
                      'shell-command-to-string)
                     (lambda (command)
                       (pcase command
                         ("audtool --current-playlist-name"
                          "Boundary\n")
                         ("audtool --current-playlist"
                          position)
                         ("audtool --number-of-playlists"
                          length))))
                    ((symbol-function 'message)
                     (lambda (text &rest _arguments)
                       (push text events)
                       text))
                    ((symbol-function
                      'audacious-playlist--goto)
                     (lambda (number)
                       (push
                        (list :unexpected-goto number)
                        events))))
                 (list
                  direction
                  (funcall direction)
                  audacious-playlist-position
                  audacious-playlist-length
                  (nreverse events))))))
         '((audacious-playlist-next
            "4\n"
            "4\n")
           (audacious-playlist-prev
            "1\n"
            "4\n")))"##,
        expect![[
            r#"OK ((audacious-playlist-next "Last playlist" 4 4 ("Last playlist")) (audacious-playlist-prev "First playlist" 1 4 ("First playlist")))"#
        ]],
    )
}

fn audacious_playlist_next_runs_complete_switch_refresh_and_song_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_playlist_next_runs_complete_switch_refresh_and_song_workflow",
        r##"(let ((answers
                '(("audtool --current-playlist-name"
                   . "Old Mix\n")
                  ("audtool --current-playlist"
                   . "2\n")
                  ("audtool --number-of-playlists"
                   . "4\n")
                  ("audtool --current-playlist-name"
                   . "New Mix\n")
                  ("audtool --current-playlist"
                   . "3\n")
                  ("audtool --number-of-playlists"
                   . "4\n")))
               events)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (let ((entry
                        (pop answers)))
                   (push
                    (list :shell command)
                    events)
                   (unless
                       (equal command
                              (car entry))
                     (error
                      "unexpected query %S"
                      command))
                   (cdr entry))))
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
              ((symbol-function 'sit-for)
               (lambda (&rest arguments)
                 (push
                  (cons :sit arguments)
                  events)))
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
                   text)))
              ((symbol-function
                'audacious-song-show-current-info)
               (lambda ()
                 (push :song-refresh events)
                 :song-refreshed)))
           (list
            (audacious-playlist-next)
            (list
             audacious-playlist-position
             audacious-playlist-length
             audacious-playlist-name)
            answers
            (nreverse events))))"##,
        expect![[
            r#"OK (:song-refreshed ("3" "4" "New Mix") nil ((:shell "audtool --current-playlist-name") (:shell "audtool --current-playlist") (:shell "audtool --number-of-playlists") (:call "/fixture/bin/audtool" nil nil nil "--set-current-playlist" "3") (:sleep 0 20) (:call "/fixture/bin/audtool" nil nil nil "--play-current-playlist") (:shell "audtool --current-playlist-name") (:shell "audtool --current-playlist") (:shell "audtool --number-of-playlists") (:message "[3/4] \"New Mix\"") (:message "[3/4] \"New Mix\"") (:sit 2) :song-refresh))"#
        ]],
    )
}

fn audacious_playlist_prev_runs_complete_switch_refresh_and_song_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_playlist_prev_runs_complete_switch_refresh_and_song_workflow",
        r##"(let ((answers
                '(("Old Mix\n"
                   "3\n"
                   "5\n")
                  ("Previous Mix\n"
                   "2\n"
                   "5\n")))
               (phase 0)
               (field 0)
               events)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (_command)
                 (let ((value
                        (nth
                         field
                         (nth phase answers))))
                   (setq field
                         (1+ field))
                   (when
                       (= field 3)
                     (setq field 0
                           phase
                           (1+ phase)))
                   value)))
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
              ((symbol-function 'sit-for)
               (lambda (&rest arguments)
                 (push
                  (cons :sit arguments)
                  events)))
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
                   text)))
              ((symbol-function
                'audacious-song-show-current-info)
               (lambda ()
                 (push :song-refresh events)
                 :song-refreshed)))
           (list
            (audacious-playlist-prev)
            (list
             audacious-playlist-position
             audacious-playlist-length
             audacious-playlist-name)
            phase
            field
            (nreverse events))))"##,
        expect![[
            r#"OK (:song-refreshed ("2" "5" "Previous Mix") 2 0 ((:call "/fixture/bin/audtool" nil nil nil "--set-current-playlist" "2") (:sleep 0 20) (:call "/fixture/bin/audtool" nil nil nil "--play-current-playlist") (:message "[2/5] \"Previous Mix\"") (:message "[2/5] \"Previous Mix\"") (:sit 2) :song-refresh))"#
        ]],
    )
}

pub(super) fn playlists_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audacious_playlist_filters_only_pipe_rows_and_resets_prior_message_state(),
        audacious_playlist_current_info_trims_queries_updates_state_and_formats_message(),
        audacious_private_playlist_goto_switches_waits_plays_and_refreshes_in_order(),
        audacious_public_playlist_goto_reads_live_length_and_executes_numeric_selection(),
        audacious_public_playlist_goto_invalid_value_reports_without_process_or_sleep(),
        audacious_playlist_next_and_prev_boundaries_report_without_switching(),
        audacious_playlist_next_runs_complete_switch_refresh_and_song_workflow(),
        audacious_playlist_prev_runs_complete_switch_refresh_and_song_workflow(),
    ]
}
