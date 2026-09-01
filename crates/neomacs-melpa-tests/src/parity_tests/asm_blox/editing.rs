use expect_test::expect;

use super::ParityBatchCase;

fn asm_blox_box_content_initialization_get_set_line_and_swap_operations_preserve_exact_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_box_content_initialization_get_set_line_and_swap_operations_preserve_exact_text",
        r##"(let ((asm-blox-box-contents nil))
         (asm-blox--initialize-box-contents)
         (asm-blox--set-box-content
          0 0
          "(const 1)\n(send right)")
         (asm-blox--set-box-content
          2 3
          "(module stack\n :input-port left)")
         (let ((before
                (list
                 (hash-table-count
                  asm-blox-box-contents)
                 (asm-blox--get-box-content 0 0)
                 (mapcar
                  (lambda (line)
                    (asm-blox--get-box-line-content
                     0 0 line))
                  '(0 1 2 11))
                 (asm-blox--get-box-content 2 3))))
           (asm-blox--swap-box-contents
            0 0 2 3)
           (list
            before
            (asm-blox--get-box-content 0 0)
            (asm-blox--get-box-content 2 3))))"##,
        expect![[
            r#"OK ((12 "(const 1)\n(send right)" ("(const 1)" "(send right)" "" "") "(module stack\n :input-port left)") "(module stack\n :input-port left)" "(const 1)\n(send right)")"#
        ]],
    )
}

fn asm_blox_practical_board_render_has_stable_geometry_labels_problem_text_and_edit_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_practical_board_render_has_stable_geometry_labels_problem_text_and_edit_properties",
        r##"(with-temp-buffer
         (asm-blox-test-prepare-edit-buffer
          '((0 0 "(get up)\n(send right)")
            (1 2 "(const 2)")
            (2 3 "(const 9)\n(send down)")))
         (let ((text
                (buffer-string)))
           (list
            (length text)
            (secure-hash 'sha256 text)
            (line-number-at-pos
             (point-max))
            (seq-position
             text ?I)
            (seq-position
             text ?O)
            (string-match-p
             "Fixture Board:"
             text)
            (string-match-p
             "Bannned Commands:"
             text)
            (mapcar
             (lambda (coords)
               (let ((point
                      (gethash
                       coords
                       asm-blox--beginning-of-box-points)))
                 (list
                  coords
                  point
                  (get-text-property
                   point
                   'asm-blox-box-id)
                  (get-text-property
                   point
                   'asm-blox-text-type))))
             '((0 0) (1 2) (2 3)))
            (hash-table-count
             asm-blox--beginning-of-box-points)
            (hash-table-count
             asm-blox--end-of-box-points))))"##,
        expect![[
            r#"OK (5839 "0f1dede3221d1d8616c9da7258d9c8e69d7e516fb4aed6359c428041463d6c30" 56 23 5744 5762 5815 (((0 0) 373 (0 0 0) nil) ((1 2) 2227 (1 2 0) nil) ((2 3) 4054 (2 3 0) nil)) 12 12)"#
        ]],
    )
}

fn asm_blox_mirror_buffer_editing_enforces_box_dimensions_and_tracks_logical_cursor()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_mirror_buffer_editing_enforces_box_dimensions_and_tracks_logical_cursor",
        r##"(with-temp-buffer
         (asm-blox-test-prepare-edit-buffer
          '((0 0 "alpha\nbeta")))
         (asm-blox--move-to-box 0 0)
         (asm-blox--move-to-box-point 0 5)
         (let (trace)
           (dolist (function
                    (list
                     (lambda ()
                       (insert "!"))
                     (lambda ()
                       (newline)
                       (insert "gamma"))
                     (lambda ()
                       (goto-char
                        (point-min)))
                     (lambda ()
                       (insert
                        (make-string 25 ?x)))))
             (asm-blox--func-in-buffer function)
             (push
              (list
               (asm-blox--get-box-content 0 0)
               (get-text-property
                (point)
                'asm-blox-box-id)
               (asm-blox-get-line-col-num)
               (line-number-at-pos)
               (current-column))
              trace))
           (nreverse trace)))"##,
        expect![[
            r#"OK (("alpha!\nbeta" (0 0 0) 6 4 18) ("alpha!\ngamma\nbeta" (0 0 1) 5 5 17) ("alpha!\ngamma\nbeta" #1=(0 0 0) 0 4 12) ("alpha!\ngamma\nbeta" #1# 0 4 12))"#
        ]],
    )
}

fn asm_blox_kill_copy_and_yank_workflows_preserve_kill_ring_and_reject_cross_box_regions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_kill_copy_and_yank_workflows_preserve_kill_ring_and_reject_cross_box_regions",
        r##"(with-temp-buffer
         (let ((kill-ring nil)
               (kill-ring-yank-pointer nil))
           (asm-blox-test-prepare-edit-buffer
            '((0 0 "alpha beta\ngamma")
              (0 1 "neighbor")))
           (asm-blox--move-to-box 0 0)
           (asm-blox--move-to-box-point 0 6)
           (let ((beginning
                  (point)))
             (asm-blox--move-to-box-point 0 10)
             (let ((end
                    (point)))
               (asm-blox-copy-region
                beginning end)
               (let ((after-copy
                      (list
                       (asm-blox--get-box-content 0 0)
                       (current-kill 0))))
                 (asm-blox-kill-region
                  beginning end)
                 (asm-blox--move-to-box 0 0)
                 (asm-blox--move-to-box-point 1 5)
                 (asm-blox-yank)
                 (let ((after-yank
                        (list
                         (asm-blox--get-box-content 0 0)
                         (current-kill 0))))
                   (asm-blox--move-to-box 0 0)
                   (let ((cross-start
                          (point)))
                     (asm-blox--move-to-box 0 1)
                     (list
                      after-copy
                      after-yank
                      (condition-case error
                          (asm-blox-kill-region
                           cross-start
                           (point))
                        (error
                         (list
                          (car error)
                          (cdr error))))))))))))"##,
        expect![[
            r#"OK (("alpha beta\ngamma" "beta") ("alpha \ngammabeta" "beta") (error ("Can’t kill region across boxes")))"#
        ]],
    )
}

fn asm_blox_cell_navigation_wraps_row_major_and_vertical_while_retaining_content_end_positions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_cell_navigation_wraps_row_major_and_vertical_while_retaining_content_end_positions",
        r##"(with-temp-buffer
         (asm-blox-test-prepare-edit-buffer
          '((0 0 "a")
            (0 1 "bb")
            (0 3 "line\nend")
            (1 3 "middle")
            (2 3 "last")))
         (asm-blox--move-to-box 0 0)
         (let (trace)
           (dotimes (_ 5)
             (asm-blox-next-cell)
             (push
              (list
               (get-text-property
                (point)
                'asm-blox-box-id)
               (asm-blox-get-line-col-num))
              trace))
           (asm-blox-prev-cell)
           (push
            (list
             :previous
             (get-text-property
              (point)
              'asm-blox-box-id)
             (asm-blox-get-line-col-num))
            trace)
           (asm-blox--next-row-cell)
           (push
            (list
             :vertical
             (get-text-property
              (point)
              'asm-blox-box-id)
             (asm-blox-get-line-col-num))
            trace)
           (nreverse trace)))"##,
        expect![
            "OK (((0 1 0) 2) ((0 2 0) 0) ((0 3 1) 3) (#1=(1 0 0) 0) ((1 1 0) 0) (:previous #1# 0) (:vertical (2 0 0) 0))"
        ],
    )
}

fn asm_blox_per_cell_undo_redo_and_stack_swapping_restore_text_and_cursor_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asm_blox_per_cell_undo_redo_and_stack_swapping_restore_text_and_cursor_state",
        r##"(with-temp-buffer
         (asm-blox-test-prepare-edit-buffer
          '((0 0 "one")
            (0 1 "two")))
         (asm-blox--initialize-undo-stacks)
         (asm-blox--move-to-box 0 0)
         (asm-blox--move-to-box-point 0 3)
         (let ((inhibit-read-only t))
           (asm-blox--func-in-buffer
            (lambda ()
              (insert "-edit"))))
         (asm-blox--push-undo-stack-value)
         (let ((edited
                (asm-blox--get-box-content 0 0)))
           (asm-blox-undo)
           (let ((undone
                  (asm-blox--get-box-content 0 0)))
             (asm-blox-redo)
             (let ((redone
                    (asm-blox--get-box-content 0 0))
                   (first-stack
                    (mapcar
                     #'asm-blox--undo-state-text
                     (gethash
                      '(0 0)
                      asm-blox--undo-stacks)))
                   (second-stack
                    (mapcar
                     #'asm-blox--undo-state-text
                     (gethash
                      '(0 1)
                      asm-blox--undo-stacks))))
               (asm-blox--swap-undo-stacks
                0 0 0 1)
               (list
                edited
                undone
                redone
                first-stack
                second-stack
                (mapcar
                 #'asm-blox--undo-state-text
                 (gethash
                  '(0 0)
                  asm-blox--undo-stacks))
                (mapcar
                 #'asm-blox--undo-state-text
                 (gethash
                  '(0 1)
                  asm-blox--undo-stacks)))))))"##,
        expect![[
            r#"OK ("one-edit" "one" "one-edit" ("one-edit" "one") ("two") ("two") ("one-edit" "one"))"#
        ]],
    )
}

fn asm_blox_nested_parenthesis_matching_and_overlay_lifecycle_follow_box_coordinates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_nested_parenthesis_matching_and_overlay_lifecycle_follow_box_coordinates",
        r##"(with-temp-buffer
         (asm-blox-test-prepare-edit-buffer
          '((1 1 "(send right\n (add (const 1)\n      (const 2)))")))
         (asm-blox--move-to-box 1 1)
         (asm-blox--move-to-box-point 0 0)
         (let ((closing
                (asm-blox--find-closing-match)))
           (asm-blox--move-to-box-point 2 16)
           (let ((opening
                  (asm-blox--find-opening-match)))
             (asm-blox--pair-create-overlays
              (point)
              (+ (point) 2))
             (let ((overlay-state
                    (mapcar
                     (lambda (overlay)
                       (list
                        (overlay-start overlay)
                        (overlay-end overlay)
                        (overlay-get overlay 'face)
                        (overlay-get overlay 'type)))
                     asm-blox-pair-overlays)))
               (asm-blox--pair-delete-overlays)
               (list
                closing
                opening
                overlay-state
                asm-blox-pair-overlays
                (overlays-in
                 (point-min)
                 (point-max)))))))"##,
        expect![
            "OK ((2 16) (-1 -15) ((2456 2457 asm-blox-show-paren-match-face show-pair) (2458 2459 asm-blox-show-paren-match-face nil)) nil nil)"
        ],
    )
}

fn asm_blox_completion_context_keyword_port_matchers_and_eldoc_support_real_nested_forms()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_completion_context_keyword_port_matchers_and_eldoc_support_real_nested_forms",
        r##"(list
         (with-temp-buffer
           (insert
            "(send ri (add (const 1) (get -1)))")
           (goto-char
            (+ (point-min) 8))
           (asm-blox--point-context))
         (with-temp-buffer
           (insert
            "(send right (add (const 1) (get -1)))")
           (goto-char (point-min))
           (let (matches)
             (while
                 (asm-blox--match-keyword
                  (point-max))
               (push
                (list
                 (match-string-no-properties 0)
                 (match-string-no-properties 1)
                 (match-beginning 1)
                 (match-end 1))
                matches))
             (nreverse matches)))
         (with-temp-buffer
           (insert
            "up diagonal LEFT right down")
           (goto-char (point-min))
           (let (matches)
             (while
                 (asm-blox--match-port
                  (point-max))
               (push
                (list
                 (match-string-no-properties 0)
                 (match-beginning 0))
                matches))
             (nreverse matches)))
         (seq-filter
          (lambda (candidate)
            (string-prefix-p
             "s"
             candidate))
          asm-blox--all-completions)
         (assoc 'SEND asm-blox-eldoc-specs))"##,
        expect![[
            r#"OK (("ri" (7 . 9) "send" 1) (("(send" "send" 2 6) ("(add" "add" 14 17) ("(const" "const" 19 24) ("(get" "get" 29 32)) (("up" 1) ("LEFT" 13) ("right" 18) ("down" 24)) ("set" "sub" "send") (SEND "POP -> X; sent X to port." port rest))"#
        ]],
    )
}

fn asm_blox_edit_and_execution_modes_install_expected_local_state_without_starting_background_timer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_edit_and_execution_modes_install_expected_local_state_without_starting_background_timer",
        r##"(let ((asm-blox--show-pair-idle-timer t))
         (list
          (with-temp-buffer
            (let ((asm-blox--skip-initial-parsing t))
              (asm-blox-mode)
              (list
               major-mode
               mode-name
               buffer-read-only
               truncate-lines
               asm-blox--display-mode
               (eq
                (current-local-map)
                asm-blox-mode-map)
               (memq
                #'asm-blox--ensure-buffer-not-empty
                write-file-functions)
               (local-variable-p
                'eldoc-documentation-functions))))
          (with-temp-buffer
            (asm-blox-execution-mode)
            (list
             major-mode
             mode-name
             buffer-read-only
             truncate-lines
             asm-blox--display-mode
             header-line-format
             asm-blox-runtime-error
             asm-blox--gameboard-state
             (eq
              (current-local-map)
              asm-blox-execution-mode-map)))))"##,
        expect![[
            r#"OK ((asm-blox-mode "asm-blox" t 0 edit t (asm-blox--ensure-buffer-not-empty t) t) (asm-blox-execution-mode "asm-blox-execution" t 0 execute "ASM-BLOX EXECUTION" nil nil t))"#
        ]],
    )
}

fn asm_blox_puzzle_selection_sorts_difficulty_renders_saved_slots_and_attaches_action_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_puzzle_selection_sorts_difficulty_renders_saved_slots_and_attaches_action_properties",
        r##"(let* ((asm-blox-save-directory-name
                 (asm-blox-test-sandbox-path
                  "selection"))
                (easy
                 (lambda ()
                   (asm-blox--problem-spec-create
                    :name "Easy Fixture"
                    :difficulty 'easy
                    :sources nil :sinks nil
                    :description
                    "A practical easy puzzle.")))
                (hard
                 (lambda ()
                   (asm-blox--problem-spec-create
                    :name "Hard Fixture"
                    :difficulty 'hard
                    :sources nil :sinks nil
                    :description
                    "A practical hard puzzle.")))
                (tutorial
                 (lambda ()
                   (asm-blox--problem-spec-create
                    :name "Tutorial Fixture"
                    :difficulty 'tutorial
                    :sources nil :sinks nil
                    :description
                    "Learn the board.")))
                (asm-blox-puzzles
                 (list hard easy tutorial)))
         (make-directory
          asm-blox-save-directory-name t)
         (with-temp-file
             (expand-file-name
              "Easy Fixture-2.asbx"
              asm-blox-save-directory-name)
           (insert "fixture"))
         (cl-letf
             (((symbol-function
                'asm-blox--puzzle-won-p)
               (lambda (name)
                 (equal
                  name
                  "Tutorial Fixture"))))
           (asm-blox-puzzle-selection-prepare-buffer)
           (with-current-buffer
               "*asm-blox-puzzle-selection*"
             (let ((text
                    (buffer-substring-no-properties
                     (point-min)
                     (point-max)))
                   properties)
               (goto-char (point-min))
               (while
                   (< (point)
                      (point-max))
                 (when-let ((id
                             (get-text-property
                              (point)
                              'asm-blox-puzzle-selection-id)))
                   (push
                    (list
                     (point)
                     id
                     (when-let ((filename
                                 (get-text-property
                                  (point)
                                  'asm-blox-puzzle-selection-filename)))
                       (file-name-nondirectory filename)))
                    properties))
                 (goto-char
                  (next-single-property-change
                   (point)
                   'asm-blox-puzzle-selection-id
                   nil
                   (point-max))))
               (list
                text
                (nreverse properties)
                (mapcar
                 (lambda (function)
                   (asm-blox--problem-spec-name
                    (funcall function)))
                 (asm-blox--puzzles-by-difficulty)))))))"##,
        expect![[
            r#"OK ("[x] tutorial Tutorial Fixture          Learn the board.                                               \n[ ] easy     Easy Fixture              A practical easy puzzle.                                       [2] \n[ ] hard     Hard Fixture              A practical hard puzzle.                                       \n" ((1 "Tutorial Fixture" nil) (104 "Easy Fixture" nil) (211 "Hard Fixture" nil)) ("Tutorial Fixture" "Easy Fixture" "Hard Fixture"))"#
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asm_blox_box_content_initialization_get_set_line_and_swap_operations_preserve_exact_text(),
        asm_blox_practical_board_render_has_stable_geometry_labels_problem_text_and_edit_properties(),
        asm_blox_mirror_buffer_editing_enforces_box_dimensions_and_tracks_logical_cursor(),
        asm_blox_kill_copy_and_yank_workflows_preserve_kill_ring_and_reject_cross_box_regions(),
        asm_blox_cell_navigation_wraps_row_major_and_vertical_while_retaining_content_end_positions(),
        asm_blox_per_cell_undo_redo_and_stack_swapping_restore_text_and_cursor_state(),
        asm_blox_nested_parenthesis_matching_and_overlay_lifecycle_follow_box_coordinates(),
        asm_blox_completion_context_keyword_port_matchers_and_eldoc_support_real_nested_forms(),
        asm_blox_edit_and_execution_modes_install_expected_local_state_without_starting_background_timer(),
        asm_blox_puzzle_selection_sorts_difficulty_renders_saved_slots_and_attaches_action_properties(),
    ]
}
