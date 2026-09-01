use expect_test::expect;

use super::ParityBatchCase;

fn audacious_integer_predicate_accepts_only_complete_signed_decimal_strings() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_integer_predicate_accepts_only_complete_signed_decimal_strings",
        r##"(list
         (mapcar
          (lambda (value)
            (list
             value
             (audacious-string-integer-p
              value)))
          '("0"
            "+0"
            "-0"
            "007"
            "+42"
            "-19"
            ""
            "+"
            " 2"
            "2 "
            "1.0"
            "1e2"
            "１２"))
         (mapcar
          (lambda (value)
            (audacious-test-error-data
             (lambda ()
               (audacious-string-integer-p
                value))))
          '(nil
            12
            integer)))"##,
        expect![[
            r#"OK ((("0" t) ("+0" t) ("-0" t) ("007" t) ("+42" t) ("-19" t) ("" nil) ("+" nil) (" 2" nil) ("2 " nil) ("1.0" nil) ("1e2" nil) ("１２" nil)) ((:error wrong-type-argument (stringp nil)) (:error wrong-type-argument (stringp 12)) (:error wrong-type-argument (stringp integer))))"#
        ]],
    )
}

fn audacious_current_song_info_queries_every_field_trims_and_updates_global_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_current_song_info_queries_every_field_trims_and_updates_global_state",
        r##"(let (events)
         (audacious-test-reset-state)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push command events)
                 (pcase command
                   ("audtool --playlist-position"
                    "  7\n")
                   ("audtool --playlist-length"
                    "12 \n")
                   ("audtool --current-song"
                    "  Artist – Song  \n")
                   ("audtool --current-song-output-length"
                    " 01:23\n")
                   ("audtool --current-song-length"
                    "03:45 \n"))))
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
            (audacious-song-show-current-info)
            (list
             audacious-playlist-position
             audacious-playlist-length
             audacious-song-title
             audacious-song-position
             audacious-song-length)
            (nreverse events))))"##,
        expect![[
            r#"OK ("[7/12]: Artist – Song [01:23 / 03:45]" ("7" "12" "Artist – Song" "01:23" "03:45") ("audtool --playlist-position" "audtool --playlist-length" "audtool --current-song" "audtool --current-song-output-length" "audtool --current-song-length" "[7/12]: Artist – Song [01:23 / 03:45]"))"#
        ]],
    )
}

fn audacious_song_goto_filters_display_builds_prompt_and_accepts_signed_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_song_goto_filters_display_builds_prompt_and_accepts_signed_index",
        r##"(let ((audacious-msg
                "stale\n")
               prompts
               events)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 "Header\n 1 | First\nnoise\n-2 | Second\nFooter\n"))
              ((symbol-function 'read-string)
               (lambda (prompt)
                 (push prompt prompts)
                 "+03"))
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
            (audacious-song-goto)
            audacious-song-position
            audacious-msg
            (nreverse prompts)
            (nreverse events))))"##,
        expect![[
            r#"OK (:refreshed "+03" "stale\n 1 | First\n-2 | Second\n" ("stale\n 1 | First\n-2 | Second\nSong No.: ") ((:shell "audtool --playlist-display") (:call "/fixture/bin/audtool" nil nil nil "--playlist-jump" "+03") (:sleep 0 20) :refresh))"#
        ]],
    )
    .fresh_process()
}

fn audacious_song_goto_invalid_input_reports_exact_value_without_playback_side_effects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_song_goto_invalid_input_reports_exact_value_without_playback_side_effects",
        r##"(let ((audacious-msg "")
               events)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (_command)
                 " 1 | One\n"))
              ((symbol-function 'read-string)
               (lambda (prompt)
                 (push
                  (list :prompt prompt)
                  events)
                 "3.5"))
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
                  events)))
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
            (audacious-song-goto)
            audacious-song-position
            audacious-msg
            (nreverse events))))"##,
        expect![[
            r#"OK ("\"3.5\" is not number." "3.5" " 1 | One\n" ((:prompt " 1 | One\nSong No.: ") (:message "\"3.5\" is not number.")))"#
        ]],
    )
}

fn audacious_song_goto_repeated_prompt_accumulates_prior_playlist_rows_by_design() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audacious_song_goto_repeated_prompt_accumulates_prior_playlist_rows_by_design",
        r##"(let ((audacious-msg "")
               (round 0)
               prompts
               messages)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (_command)
                 (setq round
                       (1+ round))
                 (format
                  "header\n%d | Song %d\nfooter\n"
                  round
                  round)))
              ((symbol-function 'read-string)
               (lambda (prompt)
                 (push prompt prompts)
                 "bad"))
              ((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (push
                  (apply
                   #'format
                   format-string
                   arguments)
                  messages))))
           (audacious-song-goto)
           (audacious-song-goto)
           (list
            audacious-msg
            (nreverse prompts)
            (nreverse messages))))"##,
        expect![[
            r#"OK ("1 | Song 1\n2 | Song 2\n" ("1 | Song 1\nSong No.: " "1 | Song 1\n2 | Song 2\nSong No.: ") ("\"bad\" is not number." "\"bad\" is not number."))"#
        ]],
    )
}

fn audacious_helm_song_selection_builds_exact_candidates_jumps_and_refreshes() -> ParityBatchCase {
    ParityBatchCase::value(
        "audacious_helm_song_selection_builds_exact_candidates_jumps_and_refreshes",
        r##"(let (events source)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (command)
                 (push
                  (list :shell command)
                  events)
                 "header\n 1 | First\n 2 | Second\nfooter\n"))
              ((symbol-function
                'helm-build-sync-source)
               (lambda (name &rest properties)
                 (setq source
                       (cons name properties))
                 source))
              ((symbol-function 'helm)
               (lambda (&rest arguments)
                 (push
                  (cons :helm arguments)
                  events)
                 " 2 | Second"))
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
            (audacious-song-goto-helm)
            source
            audacious-song-position
            (nreverse events))))"##,
        expect![[
            r#"OK (:refreshed #1=("audacious" :candidates (" 1 | First" " 2 | Second") :fuzzy-match nil) "2" ((:shell "audtool --playlist-display") (:helm :sources #1# :buffer "*helm audacious*") (:call "/fixture/bin/audtool" nil nil nil "--playlist-jump" "2") (:sleep 0 20) :refresh))"#
        ]],
    )
    .fresh_process()
}

fn audacious_helm_cancel_returns_nil_without_mutating_song_or_starting_playback() -> ParityBatchCase
{
    ParityBatchCase::value(
        "audacious_helm_cancel_returns_nil_without_mutating_song_or_starting_playback",
        r##"(let ((audacious-song-position
                :preserved)
               events)
         (cl-letf
             (((symbol-function
                'shell-command-to-string)
               (lambda (_command)
                 "header\n 1 | First\nfooter\n"))
              ((symbol-function
                'helm-build-sync-source)
               (lambda (&rest arguments)
                 (cons :source arguments)))
              ((symbol-function 'helm)
               (lambda (&rest arguments)
                 (push arguments events)
                 nil))
              ((symbol-function 'call-process)
               (lambda (&rest arguments)
                 (push
                  (cons :unexpected-call arguments)
                  events))))
           (list
            (audacious-song-goto-helm)
            audacious-song-position
            (nreverse events))))"##,
        expect![[
            r#"OK (nil :preserved ((:sources (:source "audacious" :candidates (" 1 | First") :fuzzy-match nil) :buffer "*helm audacious*")))"#
        ]],
    )
}

pub(super) fn songs_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        audacious_integer_predicate_accepts_only_complete_signed_decimal_strings(),
        audacious_current_song_info_queries_every_field_trims_and_updates_global_state(),
        audacious_song_goto_filters_display_builds_prompt_and_accepts_signed_index(),
        audacious_song_goto_invalid_input_reports_exact_value_without_playback_side_effects(),
        audacious_song_goto_repeated_prompt_accumulates_prior_playlist_rows_by_design(),
        audacious_helm_song_selection_builds_exact_candidates_jumps_and_refreshes(),
        audacious_helm_cancel_returns_nil_without_mutating_song_or_starting_playback(),
    ]
}
