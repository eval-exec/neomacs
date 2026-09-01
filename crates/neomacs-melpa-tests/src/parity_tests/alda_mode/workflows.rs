use expect_test::expect;

use super::ParityBatchCase;

/// The package's central action: select a phrase in a real `.alda' score and
/// play it.  `alda-play-region' composes the command, `alda-run-cmd' starts the
/// process and the Alda CLI receives `play -F "" --code <score>' -- the empty
/// `-F' being what alda-mode passes when no history has been accumulated.
///
/// Nothing inside alda-mode is redefined here, so the assertion is the vector
/// that actually crossed out of Emacs, and the output buffer holds what Alda
/// 2.3.2 really printed for it, on the stream it really used: "Starting player
/// processes...\nPlaying...\n" arrives on *stderr*, which `start-process'
/// merges into the buffer the user reads.
fn playing_a_selected_phrase_sends_the_score_to_the_alda_binary_and_shows_its_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "playing_a_selected_phrase_sends_the_score_to_the_alda_binary_and_shows_its_output",
        r##"(progn
  (alda-test-install-alda)
  (let* ((buffer (alda-test-score-buffer "melody.alda" "piano: o4 c d e\n"))
         (*alda-history* ""))
    (list :mode major-mode
          :discovered-binary
          (file-name-nondirectory (or (alda-location) "none"))
          :played (progn
                    (alda-play-region (point-min) (1- (point-max)))
                    (alda-test-settle 20)
                    (alda-test-calls))
          :output (alda-test-output)
          :unrecorded (alda-test-unrecorded))))"##,
        expect![[
            r#"OK (:mode alda-mode :discovered-binary "alda" :played ("play|-F||--code|piano: o4 c d e|") :output "Playing...\n\nProcess alda-playback finished\n" :unrecorded nil)"#
        ]],
    )
}

fn appending_to_history_then_playing_seeks_past_the_accumulated_score_to_a_marker()
-> ParityBatchCase {
    ParityBatchCase::value(
        "appending_to_history_then_playing_seeks_past_the_accumulated_score_to_a_marker",
        r##"(progn
  (alda-test-install-alda)
  (let* ((buffer (alda-test-score-buffer "session.alda" "piano:\n  o4 c d e\n"))
         (*alda-history* ""))
    (alda-history-append-buffer)
    (let ((accumulated (copy-sequence *alda-history*)))
      (alda-play-text "f g a")
      (alda-test-settle 20)
      (list :history accumulated
            :calls (alda-test-calls)
            :output (alda-test-output)
            :unrecorded (alda-test-unrecorded)))))"##,
        expect![[
            r#"OK (:history "\npiano:\n  o4 c d e\n" :calls ("play|-F|alda-mode-internal-marker|--code|~piano:~  o4 c d e~~%alda-mode-internal-marker~f g a|") :output "Playing...\n\nProcess alda-playback finished\n" :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

fn playing_the_whole_file_then_stopping_uses_the_clis_real_file_and_stop_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "playing_the_whole_file_then_stopping_uses_the_clis_real_file_and_stop_commands",
        r##"(progn
  (alda-test-install-alda)
  (let* ((buffer (alda-test-score-buffer "score.alda" "piano: o4 c d e f g\n"))
         (*alda-history* ""))
    (alda-play-file)
    (alda-test-settle 20)
    (alda-stop)
    (alda-test-settle 20)
    (list :calls (mapcar #'file-name-nondirectory (alda-test-calls))
          :output (alda-test-output)
          :unrecorded (alda-test-unrecorded))))"##,
        expect![[
            r#"OK (:calls ("score.alda|" "stop|") :output "Starting player processes...\nPlaying...\n\nProcess alda-playback finished\nStopping playback.\n\nProcess alda-playback finished\n" :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

fn alda_down_invokes_a_subcommand_the_alda_cli_does_not_have() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_down_invokes_a_subcommand_the_alda_cli_does_not_have",
        r##"(progn
  (alda-test-install-alda)
  (let ((buffer (alda-test-score-buffer "down.alda" "piano: c\n")))
    (list :composed-command (concat (file-name-nondirectory (alda-location)) " down")
          :shell-exit (shell-command (concat (alda-location) " down"))
          :calls (alda-test-calls)
          :banner-first-lines
          (with-current-buffer "*Shell Command Output*"
            (seq-take (split-string (buffer-string) "\n") 3))
          :unrecorded (alda-test-unrecorded))))"##,
        expect![[
            r#"OK (:composed-command "alda down" :shell-exit 1 :calls ("down|") :banner-first-lines ("Usage:" "  alda [command]" "") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

fn the_binary_is_taken_from_the_option_then_exec_path_and_refused_when_absent() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_binary_is_taken_from_the_option_then_exec_path_and_refused_when_absent",
        r##"(progn
  (alda-test-install-alda)
  (let ((observed nil)
        (mark (with-current-buffer (get-buffer-create "*Messages*") (point-max))))
    (let ((alda-binary-location "/opt/alda/bin/alda"))
      (push (list :from-the-option
                  (list :location (alda-location) :repl (alda-repl)))
            observed))
    (let ((alda-binary-location nil))
      (push (list :from-exec-path
                  (list :location (file-name-nondirectory (alda-location))
                        :repl (file-name-nondirectory (alda-repl))))
            observed))
    (let ((alda-binary-location nil)
          (exec-path (list "/nonexistent-directory-for-alda"))
          (before (alda-test-calls)))
      (alda-run-cmd "play" "--code" "piano: c")
      (alda-test-settle 5)
      (push (list :with-no-binary-anywhere
                  (list :location (alda-location)
                        :no-new-calls (equal (alda-test-calls) before)
                        :message
                        (with-current-buffer "*Messages*"
                          (car (last (split-string
                                      (buffer-substring-no-properties
                                       (min mark (point-max)) (point-max))
                                      "\n" t))))))
            observed))
    (push (list :unrecorded (alda-test-unrecorded)) observed)
    (nreverse observed)))"##,
        expect![[
            r#"OK ((:from-the-option (:location "/opt/alda/bin/alda" :repl "/opt/alda/bin/alda repl")) (:from-exec-path (:location "alda" :repl "alda repl")) (:with-no-binary-anywhere (:location nil :no-new-calls t :message "Alda was not found on your $PATH and alda-binary-location was nil.")) (:unrecorded nil))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        playing_a_selected_phrase_sends_the_score_to_the_alda_binary_and_shows_its_output(),
        appending_to_history_then_playing_seeks_past_the_accumulated_score_to_a_marker(),
        playing_the_whole_file_then_stopping_uses_the_clis_real_file_and_stop_commands(),
        alda_down_invokes_a_subcommand_the_alda_cli_does_not_have(),
        the_binary_is_taken_from_the_option_then_exec_path_and_refused_when_absent(),
    ]
}
