use expect_test::expect;

use super::ParityBatchCase;

fn audacious_practical_playback_session_updates_backend_and_reports_each_transition()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_practical_playback_session_updates_backend_and_reports_each_transition",
        r##"(let ((status "")
               (volume 60)
               (playlist-position 1)
               (playlist-length 3)
               (song-title "Opening")
               (song-output "00:00")
               (song-length "03:00")
               events)
         (audacious-test-reset-state)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 (pcase command
                   ("audtool --playback-status"
                    status)
                   ("audtool --get-volume"
                    (format "%d%%\n" volume))
                   ("audtool --playlist-position"
                    (format
                     "%d\n"
                     playlist-position))
                   ("audtool --playlist-length"
                    (format
                     "%d\n"
                     playlist-length))
                   ("audtool --current-song"
                    (concat song-title "\n"))
                   ("audtool --current-song-output-length"
                    (concat song-output "\n"))
                   ("audtool --current-song-length"
                    (concat song-length "\n")))))
              ((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :call arguments)
                  events)
                 (let ((program
                        (car arguments))
                       (operation
                        (nth 4 arguments))
                       (value
                        (nth 5 arguments)))
                   (cond
                    ((equal program "audacious")
                     (setq status "stopped\n"))
                    ((equal operation
                            "--playback-play")
                     (setq status "playing\n"))
                    ((equal operation
                            "--playback-pause")
                     (setq status "paused\n"))
                    ((equal operation
                            "--playback-stop")
                     (setq status "stopped\n"))
                    ((equal operation
                            "--set-volume")
                     (setq volume
                           (+ volume
                              (string-to-number
                               value))))
                    ((equal operation
                            "--playback-seek-relative")
                     (setq song-output "00:10"))
                    ((equal operation
                            "--playlist-advance")
                     (setq playlist-position 2
                           song-title "Second"
                           song-output "00:00")))
                   0)))
              ((symbol-function 'sleep-for)
               (lambda (&rest arguments)
                 (push
                  (cons :sleep arguments)
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
                   text))))
           (list
            (audacious-play)
            (audacious-volume-up)
            (audacious-song-seek-forward)
            (audacious-song-next)
            (audacious-pause)
            (audacious-stop)
            (list
             status
             volume
             playlist-position
             song-title
             song-output)
            (list
             audacious-playlist-position
             audacious-playlist-length
             audacious-song-title
             audacious-song-position
             audacious-song-length)
            (nreverse events))))"##,
        expect![[
            r#"OK ("[1/3]: Opening [00:00 / 03:00]" "70%" "[1/3]: Opening [00:10 / 03:00]" "[2/3]: Second [00:00 / 03:00]" "paused" 0 ("stopped\n" 70 2 "Second" "00:00") ("2" "3" "Second" "00:00" "03:00") ((:shell "audtool --playback-status") (:call "audacious" nil 0 nil "-H" "2>/dev/null") (:call "/fixture/bin/audtool" nil nil nil "--playback-play") (:sleep 0 20) (:shell "audtool --playlist-position") (:shell "audtool --playlist-length") (:shell "audtool --current-song") (:shell "audtool --current-song-output-length") (:shell "audtool --current-song-length") (:message "[1/3]: Opening [00:00 / 03:00]") (:call "/fixture/bin/audtool" nil nil nil "--set-volume" "+10%") (:shell "audtool --get-volume") (:message "70%") (:call "/fixture/bin/audtool" nil nil nil "--playback-seek-relative" "+10") (:shell "audtool --playlist-position") (:shell "audtool --playlist-length") (:shell "audtool --current-song") (:shell "audtool --current-song-output-length") (:shell "audtool --current-song-length") (:message "[1/3]: Opening [00:10 / 03:00]") (:call "/fixture/bin/audtool" nil nil nil "--playlist-advance") (:sleep 0 20) (:shell "audtool --playlist-position") (:shell "audtool --playlist-length") (:shell "audtool --current-song") (:shell "audtool --current-song-output-length") (:shell "audtool --current-song-length") (:message "[2/3]: Second [00:00 / 03:00]") (:call "/fixture/bin/audtool" nil nil nil "--playback-pause") (:shell "audtool --playback-status") (:message "paused") (:call "/fixture/bin/audtool" nil nil nil "--playback-stop")))"#
        ]],
    )
    .fresh_process()
}

fn audacious_playlist_then_song_selection_forms_one_ordered_user_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_playlist_then_song_selection_forms_one_ordered_user_workflow",
        r##"(let ((answers
                '("2"
                  "4"))
               events)
         (audacious-test-reset-state)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 (pcase command
                   ("audtool --number-of-playlists"
                    "5\n")
                   ("audtool --playlist-display"
                    "header\n 1 | One\n 4 | Four\nfooter\n"))))
              ((symbol-function 'read-string)
               (lambda (prompt)
                 (push
                  (list :prompt prompt)
                  events)
                 (pop answers)))
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
                 (push :playlist-refresh events)
                 :playlist-refreshed))
              ((symbol-function
                'audacious-song-show-current-info)
               (lambda ()
                 (push :song-refresh events)
                 :song-refreshed)))
           (list
            (audacious-playlist-goto)
            (audacious-song-goto)
            audacious-playlist-position
            audacious-song-position
            audacious-msg
            answers
            (nreverse events))))"##,
        expect![[
            r#"OK (:playlist-refreshed :song-refreshed "2" "4" " 1 | One\n 4 | Four\n" nil ((:shell "audtool --number-of-playlists") (:prompt "Playlist No. [1 - 5]: ") (:call "/fixture/bin/audtool" nil nil nil "--set-current-playlist" "2") (:sleep 0 20) (:call "/fixture/bin/audtool" nil nil nil "--play-current-playlist") :playlist-refresh (:shell "audtool --playlist-display") (:prompt " 1 | One\n 4 | Four\nSong No.: ") (:call "/fixture/bin/audtool" nil nil nil "--playlist-jump" "4") (:sleep 0 20) :song-refresh))"#
        ]],
    )
    .fresh_process()
}

fn audacious_playback_process_failure_propagates_before_sleep_or_refresh() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_playback_process_failure_propagates_before_sleep_or_refresh",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 "stopped\n"))
              ((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :call arguments)
                  events)
                 (error
                  "fixture process failure")))
              ((symbol-function 'sleep-for)
               (lambda (&rest arguments)
                 (push
                  (cons :unexpected-sleep arguments)
                  events)))
              ((symbol-function
                'audacious-song-show-current-info)
               (lambda ()
                 (push :unexpected-refresh events))))
           (list
            (audacious-test-error-data
             #'audacious-play)
            (nreverse events))))"##,
        expect![[
            r#"OK ((:error error ("fixture process failure")) ((:shell "audtool --playback-status") (:call "/fixture/bin/audtool" nil nil nil "--playback-play")))"#
        ]],
    )
    .fresh_process()
}

fn audacious_custom_command_controls_process_calls_while_queries_keep_upstream_cli_literal()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_custom_command_controls_process_calls_while_queries_keep_upstream_cli_literal",
        r##"(let ((audacious-command
                "/opt/player/bin/audtool")
               events)
         (cl-letf
             (((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :call arguments)
                  events)
                 0))
              ((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 " 88% \n"))
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
            (audacious-volume "-10%")
            (audacious-volume-up)
            (nreverse events))))"##,
        expect![[
            r#"OK (0 "88%" ((:call "/opt/player/bin/audtool" nil nil nil "--set-volume" "-10%") (:call "/opt/player/bin/audtool" nil nil nil "--set-volume" "+10%") (:shell "audtool --get-volume") (:message "88%")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audacious_practical_playback_session_updates_backend_and_reports_each_transition(),
        audacious_playlist_then_song_selection_forms_one_ordered_user_workflow(),
        audacious_playback_process_failure_propagates_before_sleep_or_refresh(),
        audacious_custom_command_controls_process_calls_while_queries_keep_upstream_cli_literal(),
    ]
}
