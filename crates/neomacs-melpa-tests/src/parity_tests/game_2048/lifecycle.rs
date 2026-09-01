use expect_test::expect;

use super::ParityBatchCase;

fn game_2048_init_resets_state_and_inserts_two_deterministic_tiles() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_init_resets_state_and_inserts_two_deterministic_tiles",
        r##"(let ((*2048-columns* 2)
                     (*2048-rows* 2)
                     (*2048-board*
                      (vector 8))
                     (*2048-score* 99)
                     (*2048-hi-tile* 64)
                     (*2048-victory-value* 128)
                     (*2048-default-victory-value*
                      32)
                     (*2048-game-has-been-added-to-history*
                      t)
                     (*2048-possible-values-to-insert*
                      [4 2])
                     (random-values
                      '(1 0 0 1 0 1))
                     events)
               (cl-letf (((symbol-function 'random)
                          (lambda (_)
                            (prog1
                                (car random-values)
                              (setq random-values
                                    (cdr random-values)))))
                         ((symbol-function 'current-time)
                          (lambda ()
                            '(123 456 0 0)))
                         ((symbol-function
                           '2048-init-tiles)
                          (lambda ()
                            (push 'tiles events)))
                         ((symbol-function
                           '2048-print-board)
                          (lambda ()
                            (push 'print events)))
                         ((symbol-function 'message)
                          (lambda (text &rest _)
                            (push
                             (list 'message text)
                             events)
                            text)))
                 (list
                  (2048-init)
                  *2048-board*
                  *2048-combines-this-move*
                  *2048-score*
                  *2048-hi-tile*
                  *2048-victory-value*
                  *2048-game-has-been-added-to-history*
                  *2048-game-epoch*
                  (nreverse events))))"##,
        expect![[
            r#"OK ("Good luck!" [2 2 0 0] [nil nil nil nil] 0 2 32 nil (123 456 0 0) (tiles print (message "Good luck!")))"#
        ]],
    )
}

fn game_2048_entry_command_switches_disables_undo_sets_mode_and_initializes() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_entry_command_switches_disables_undo_sets_mode_and_initializes",
        r##"(let (events)
               (cl-letf (((symbol-function
                           'switch-to-buffer)
                          (lambda (buffer &rest _)
                            (push
                             (list 'switch buffer)
                             events)))
                         ((symbol-function
                           'buffer-disable-undo)
                          (lambda (&optional buffer)
                            (push
                             (list
                              'disable-undo
                              buffer)
                             events)))
                         ((symbol-function '2048-mode)
                          (lambda ()
                            (push 'mode events)))
                         ((symbol-function '2048-init)
                          (lambda ()
                            (push 'init events)
                            'initialized)))
                 (list
                  (2048-game)
                  (nreverse events))))"##,
        expect![[r#"OK (initialized ((switch "2048") (disable-undo "2048") mode init))"#]],
    )
}

fn game_2048_history_sorts_truncates_and_uses_current_global_score_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_history_sorts_truncates_and_uses_current_global_score_values",
        r##"(let ((*2048-score* 42)
                     (*2048-hi-tile* 8)
                     (*2048-history*
                      '((100 16 "old-a" 1)
                        (20 4 "old-b" 2)
                        (80 8 "old-c" 3)))
                     (*2048-history-size* 3))
               (2048-add-new-history-item
                999
                1024
                (encode-time
                 0 0 0 2 1 2020 t)
                65)
               *2048-history*)"##,
        expect![[r#"OK ((100 16 "old-a" 1) (80 8 "old-c" 3) (42 8 "2020-01-02" 65))"#]],
    )
}

fn game_2048_winning_continue_doubles_the_next_victory_target() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_winning_continue_doubles_the_next_victory_target",
        r##"(let ((*2048-score* 100)
                     (*2048-hi-tile* 2048)
                     (*2048-victory-value* 2048)
                     events)
               (cl-letf (((symbol-function
                           '2048-game-was-won)
                          (lambda () t))
                         ((symbol-function
                           '2048-game-was-lost)
                          (lambda ()
                            (error
                             "loss check should not run")))
                         ((symbol-function
                           '2048-print-board)
                          (lambda ()
                            (push 'print events)))
                         ((symbol-function 'y-or-n-p)
                          (lambda (prompt)
                            (push
                             (list 'prompt prompt)
                             events)
                            nil)))
                 (list
                  (2048-check-game-end)
                  *2048-victory-value*
                  (nreverse events))))"##,
        expect![[
            r#"OK (4096 4096 (print (prompt "Yay! You beat the game!  y to start again; n to continue.  Start again? ")))"#
        ]],
    )
}

fn game_2048_winning_restart_records_history_then_initializes() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_winning_restart_records_history_then_initializes",
        r##"(let ((*2048-score* 100)
                     (*2048-hi-tile* 2048)
                     (*2048-game-epoch* '(1 0 0 0))
                     (times
                      '((2 0 0 0)
                        (3 0 0 0)))
                     events)
               (cl-letf (((symbol-function
                           '2048-game-was-won)
                          (lambda () t))
                         ((symbol-function
                           '2048-print-board)
                          #'ignore)
                         ((symbol-function 'y-or-n-p)
                          (lambda (_) t))
                         ((symbol-function 'current-time)
                          (lambda ()
                            (prog1
                                (car times)
                              (setq times
                                    (cdr times)))))
                         ((symbol-function
                           '2048-add-new-history-item)
                          (lambda (&rest args)
                            (push
                             (cons 'history args)
                             events)))
                         ((symbol-function '2048-init)
                          (lambda ()
                            (push 'init events)
                            'initialized)))
                 (list
                  (2048-check-game-end)
                  (nreverse events))))"##,
        expect!["OK (initialized ((history 100 2048 (2 0 0 0) (2 0 0 0)) init))"],
    )
}

fn game_2048_loss_records_history_only_once_without_restart() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_loss_records_history_only_once_without_restart",
        r##"(let ((*2048-score* 50)
                     (*2048-hi-tile* 128)
                     (*2048-game-epoch* '(1 0 0 0))
                     (*2048-game-has-been-added-to-history*
                      nil)
                     events)
               (cl-letf (((symbol-function
                           '2048-game-was-won)
                          (lambda () nil))
                         ((symbol-function
                           '2048-game-was-lost)
                          (lambda () t))
                         ((symbol-function
                           '2048-print-board)
                          (lambda ()
                            (push 'print events)))
                         ((symbol-function 'y-or-n-p)
                          (lambda (prompt)
                            (push
                             (list 'prompt prompt)
                             events)
                            nil))
                         ((symbol-function 'current-time)
                          (lambda ()
                            '(2 0 0 0)))
                         ((symbol-function
                           '2048-add-new-history-item)
                          (lambda (&rest args)
                            (push
                             (cons 'history args)
                             events))))
                 (2048-check-game-end)
                 (2048-check-game-end)
                 (list
                  *2048-game-has-been-added-to-history*
                  (nreverse events))))"##,
        expect![[
            r#"OK (t ((history 50 128 (2 0 0 0) (1 0 0 0)) print (prompt "Aw, too bad.  You lost.  Want to play again? ") print (prompt "Aw, too bad.  You lost.  Want to play again? ")))"#
        ]],
    )
}

fn game_2048_board_renderer_outputs_grid_score_help_and_history() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_board_renderer_outputs_grid_score_help_and_history",
        r##"(let ((*2048-columns* 2)
                     (*2048-rows* 1)
                     (*2048-board*
                      (vector 2 0))
                     (*2048-score* 4)
                     (*2048-history* nil))
               (2048-init-tiles)
               (with-temp-buffer
                 (2048-print-board)
                 (let ((text (buffer-string)))
                   (list
                    (point)
                    (substring-no-properties
                     text)
                    (get-text-property
                     19 'font-lock-face
                     text)))))"##,
        expect![[
            r#"OK (1 "+-------+-------+\n|       |       |\n|    2  |       |\n|       |       |\n+-------+-------+\n\n         /==========\\\n         | Score: 4 |\n         \\==========/\n\nThe goal is to create a tile with value 2048.\nUse the arrow keys, p/n/b/f, or C-p/C-n/C-b/C-f\nto move the tiles around. Press r to move randomly.\n\nIf two tiles of the same value collide, the tiles\ncombine into a tile with twice the value.\n\n         /=============\\\n         | HIGH SCORES |\n         \\=============/\n\n   Score  Hi-Tile     Date     Duration\n" twentyfortyeight-face-2)"#
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        game_2048_init_resets_state_and_inserts_two_deterministic_tiles(),
        game_2048_entry_command_switches_disables_undo_sets_mode_and_initializes(),
        game_2048_history_sorts_truncates_and_uses_current_global_score_values(),
        game_2048_winning_continue_doubles_the_next_victory_target(),
        game_2048_winning_restart_records_history_then_initializes(),
        game_2048_loss_records_history_only_once_without_restart(),
        game_2048_board_renderer_outputs_grid_score_help_and_history(),
    ]
}
