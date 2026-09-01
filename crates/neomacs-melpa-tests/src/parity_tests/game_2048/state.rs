use expect_test::expect;

use super::ParityBatchCase;

fn game_2048_public_defaults_and_mode_bindings_match_the_pinned_release() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_public_defaults_and_mode_bindings_match_the_pinned_release",
        r##"(with-temp-buffer
               (2048-mode)
               (list
                *2048-board*
                *2048-columns*
                *2048-rows*
                *2048-possible-values-to-insert*
                *2048-victory-value*
                *2048-default-victory-value*
                *2048-debug*
                *2048-numbers*
                *2048-score*
                *2048-hi-tile*
                *2048-history*
                *2048-history-size*
                major-mode
                mode-name
                (mapcar
                 (lambda (key)
                   (lookup-key
                    2048-mode-map
                    (kbd key)))
                 '("p" "C-p" "<up>"
                   "n" "C-n" "<down>"
                   "b" "C-b" "<left>"
                   "f" "C-f" "<right>"
                   "r"))))"##,
        expect![[
            r#"OK (nil 4 4 (4 2 2 2 2 2 2 2 2 2) nil 2048 nil (0 2 4 8 16 32 64 128 256 512 1024 2048) nil nil nil 10 2048-mode "2048-mode" (2048-up 2048-up 2048-up 2048-down 2048-down 2048-down 2048-left 2048-left 2048-left 2048-right 2048-right 2048-right 2048-random-move))"#
        ]],
    )
}

fn game_2048_cell_access_updates_flat_board_and_high_tile() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_cell_access_updates_flat_board_and_high_tile",
        r##"(let ((*2048-columns* 3)
                     (*2048-rows* 2)
                     (*2048-board*
                      (vector 0 2 4 8 16 32))
                     (*2048-hi-tile* 4))
               (list
                (2048-get-cell 0 0)
                (2048-get-cell 1 2)
                (2048-set-cell 0 1 64)
                (copy-sequence *2048-board*)
                *2048-hi-tile*
                (2048-set-cell 1 0 2)
                (copy-sequence *2048-board*)
                *2048-hi-tile*))"##,
        expect!["OK (0 32 64 [0 64 4 8 16 32] 64 2 [0 64 4 2 16 32] 64)"],
    )
}

fn game_2048_bounds_cover_every_edge_and_reject_outside_coordinates() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_bounds_cover_every_edge_and_reject_outside_coordinates",
        r##"(let ((*2048-columns* 3)
                     (*2048-rows* 2))
               (mapcar
                (lambda (coordinate)
                  (apply #'in-bounds
                         coordinate))
                '((0 0) (0 2) (1 0) (1 2)
                  (-1 0) (0 -1) (2 0) (0 3))))"##,
        expect!["OK (t t t t nil nil nil nil)"],
    )
}

fn game_2048_combination_flags_use_the_same_flat_indexing() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_combination_flags_use_the_same_flat_indexing",
        r##"(let ((*2048-columns* 3)
                     (*2048-rows* 2)
                     (*2048-combines-this-move*
                      (make-vector 6 nil)))
               (list
                (2048-was-combined-this-turn
                 1 2)
                (2048-set-was-combined-this-turn
                 1 2)
                *2048-combines-this-move*
                (2048-was-combined-this-turn
                 1 2)))"##,
        expect!["OK (nil t [nil nil nil nil nil t] t)"],
    )
}

fn game_2048_tile_symbols_printable_values_and_faces_cover_known_and_large_tiles() -> ParityBatchCase
{
    ParityBatchCase::value(
        "game_2048_tile_symbols_printable_values_and_faces_cover_known_and_large_tiles",
        r##"(list
               (mapcar
                #'2048-num-to-printable
                '(0 2 2048 4096))
               (mapcar
                #'2048-empty-symbol
                '(0 2 4096))
               (mapcar
                #'2048-tile-symbol
                '(0 2 4096))
               (mapcar
                #'2048-get-face-symbol
                '(2 2048 4096))
               (mapcar
                #'2048-get-face
                '(2 2048 4096)))"##,
        expect![[
            r#"OK (("" "2" "2048" "4096") (2048-empty-0 2048-empty-2 2048-empty-4096) (2048-tile-0 2048-tile-2 2048-tile-4096) (twentyfortyeight-face-2 twentyfortyeight-face-2048 twentyfortyeight-face-4096) (twentyfortyeight-face-2 twentyfortyeight-face-2048 twentyfortyeight-face-2048))"#
        ]],
    )
}

fn game_2048_tile_initialization_sets_width_text_and_face_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_tile_initialization_sets_width_text_and_face_properties",
        r##"(progn
               (2048-init-tile 0)
               (2048-init-tile 2)
               (2048-init-tile 4096)
               (list
                (mapcar
                 (lambda (number)
                   (list
                    (2048-empty-tile number)
                    (2048-tile number)
                    (length
                     (2048-empty-tile number))
                    (length
                     (2048-tile number))
                    (get-text-property
                     0 'font-lock-face
                     (2048-tile number))))
                 '(0 2 4096))))"##,
        expect![[
            r#"OK ((("       " "       " 7 7 nil) (#("       " 0 7 (font-lock-face twentyfortyeight-face-2)) #("    2  " 0 7 (font-lock-face twentyfortyeight-face-2)) 7 7 twentyfortyeight-face-2) (#("       " 0 7 (font-lock-face twentyfortyeight-face-2048)) #(" 4096  " 0 7 (font-lock-face twentyfortyeight-face-2048)) 7 7 twentyfortyeight-face-2048)))"#
        ]],
    )
}

fn game_2048_random_insertion_retries_occupied_cells_with_exact_random_bounds() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_random_insertion_retries_occupied_cells_with_exact_random_bounds",
        r##"(let ((*2048-columns* 2)
                     (*2048-rows* 2)
                     (*2048-board*
                      (vector 2 0 0 0))
                     (*2048-hi-tile* 2)
                     (*2048-possible-values-to-insert*
                      [4 2])
                     (values '(0 0 0 1 1))
                     calls)
               (cl-letf (((symbol-function 'random)
                          (lambda (limit)
                            (push limit calls)
                            (prog1
                                (car values)
                              (setq values
                                    (cdr values))))))
                 (list
                  (2048-insert-random-cell)
                  *2048-board*
                  *2048-hi-tile*
                  (nreverse calls))))"##,
        expect!["OK (4 [2 0 0 4] 4 (2 2 2 2 2))"],
    )
}

fn game_2048_win_and_loss_detection_cover_open_mergeable_and_blocked_boards() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_win_and_loss_detection_cover_open_mergeable_and_blocked_boards",
        r##"(let ((*2048-columns* 2)
                     (*2048-rows* 2)
                     (*2048-victory-value* 16))
               (mapcar
                (lambda (board)
                  (let ((*2048-board*
                         (vconcat board)))
                    (list
                     (2048-game-was-won)
                     (2048-game-was-lost))))
                '((2 4 8 16)
                  (2 4 8 0)
                  (2 2 4 8)
                  (2 4 8 32))))"##,
        expect!["OK ((t t) (nil nil) (nil nil) (nil t))"],
    )
}

fn game_2048_debug_macro_writes_only_when_enabled() -> ParityBatchCase {
    ParityBatchCase::value(
        "game_2048_debug_macro_writes_only_when_enabled",
        r##"(let ((*2048-debug* nil))
               (when (get-buffer "2048-debug")
                 (kill-buffer "2048-debug"))
               (unwind-protect
                   (progn
                     (2048-debug "hidden" " message")
                     (let ((before
                            (get-buffer
                             "2048-debug")))
                       (setq *2048-debug* t)
                       (2048-debug
                        "shown " "message")
                       (list
                        before
                        (with-current-buffer
                            "2048-debug"
                          (buffer-string)))))
                 (when
                     (get-buffer "2048-debug")
                   (kill-buffer
                    "2048-debug"))))"##,
        expect![[r#"OK (nil "\n\"shown message\"\n")"#]],
    )
}

pub(super) fn state_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        game_2048_public_defaults_and_mode_bindings_match_the_pinned_release(),
        game_2048_cell_access_updates_flat_board_and_high_tile(),
        game_2048_bounds_cover_every_edge_and_reject_outside_coordinates(),
        game_2048_combination_flags_use_the_same_flat_indexing(),
        game_2048_tile_symbols_printable_values_and_faces_cover_known_and_large_tiles(),
        game_2048_tile_initialization_sets_width_text_and_face_properties(),
        game_2048_random_insertion_retries_occupied_cells_with_exact_random_bounds(),
        game_2048_win_and_loss_detection_cover_open_mergeable_and_blocked_boards(),
        game_2048_debug_macro_writes_only_when_enabled(),
    ]
}
